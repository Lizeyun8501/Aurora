package com.aurora.note;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.util.Log;

/**
 * 网络状态中继 — DK-08 §7.3「仅 Wi-Fi 同步」平台源（Android 工程层接线）。
 *
 * 职责：ConnectivityManager 默认网络回调 → 三态映射 → JNI 推送
 * `SyncEngine.nativeUpdateNetworkState`（Rust 侧 AndroidNetworkState
 * AtomicU8 缓存，SyncGate 消费）。
 *
 * 状态约定（与 mobile-ffi p2p_sync.rs 对齐，越界值 Rust 侧归零放行）：
 *   0 = NOT_METERED（Wi-Fi / 以太网，允许同步）
 *   1 = METERED（蜂窝等计量网络，wifi_only=ON 时推迟）
 *   2 = Offline（默认网络丢失，恒推迟）
 *
 * 时序自愈：Wi-Fi→蜂窝切换时 onLost(旧网络) 可能晚于新网络的
 * onCapabilitiesChanged 到达——onLost 不盲目推 Offline，而是重探
 * activeNetwork（null 才是真正离线），避免错误状态滞留。
 *
 * 生命周期：MainActivity onCreate 注册 / onDestroy 注销；
 * register 内部自探初始状态（首回调前即得真实值，不等首次网络事件）。
 */
public final class NetworkStateRelay {

    private static final String TAG = "NetworkStateRelay";

    /** 状态常量（与 Rust 侧 JNI 约定一一对应）。 */
    public static final int STATE_NOT_METERED = 0;
    public static final int STATE_METERED = 1;
    public static final int STATE_OFFLINE = 2;

    private final ConnectivityManager cm;
    private final ConnectivityManager.NetworkCallback callback;
    private boolean registered = false;

    public NetworkStateRelay(Context context) {
        cm = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        callback = new ConnectivityManager.NetworkCallback() {
            @Override
            public void onCapabilitiesChanged(Network network,
                                              NetworkCapabilities capabilities) {
                push(capabilities);
            }

            @Override
            public void onLost(Network network) {
                // 竞态自愈：重探当前默认网络（见类注释），仅真离线才推 Offline
                NetworkCapabilities caps =
                        cm.getNetworkCapabilities(cm.getActiveNetwork());
                if (caps == null) {
                    SyncEngine.updateNetworkState(STATE_OFFLINE);
                } else {
                    push(caps);
                }
            }
        };
    }

    /** 注册回调并立即推送一次当前真实状态（minSdk 24：default callback 可用）。 */
    public synchronized void register() {
        if (registered || cm == null) return;
        try {
            cm.registerDefaultNetworkCallback(callback);
            registered = true;
            NetworkCapabilities caps =
                    cm.getNetworkCapabilities(cm.getActiveNetwork());
            if (caps == null) {
                SyncEngine.updateNetworkState(STATE_OFFLINE);
            } else {
                push(caps);
            }
        } catch (Exception e) {
            // 注册失败保持 Rust 侧初值 Unmetered（放行不误伤），不阻断主流程
            Log.w(TAG, "registerNetworkCallback failed: " + e.getMessage());
        }
    }

    /** 注销回调（幂等；异常吞掉防 onDestroy 崩溃）。 */
    public synchronized void unregister() {
        if (!registered || cm == null) return;
        try {
            cm.unregisterNetworkCallback(callback);
        } catch (Exception e) {
            Log.w(TAG, "unregisterNetworkCallback failed: " + e.getMessage());
        } finally {
            registered = false;
        }
    }

    /** 计量判定推送：NOT_METERED capability → 0 / 否则 1。 */
    private void push(NetworkCapabilities caps) {
        boolean unmetered = caps.hasCapability(
                NetworkCapabilities.NET_CAPABILITY_NOT_METERED);
        SyncEngine.updateNetworkState(unmetered ? STATE_NOT_METERED : STATE_METERED);
    }
}
