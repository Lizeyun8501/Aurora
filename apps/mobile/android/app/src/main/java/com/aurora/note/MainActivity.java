package com.aurora.note;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.os.Bundle;
import android.view.View;
import android.webkit.JavascriptInterface;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.Toast;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.List;

/**
 * WebView 容器 — V19 §36.3 Capacitor+React 正式架构。
 *
 * 数据链路: React (assets/index.html)
 *         → window.AndroidBridge (@JavascriptInterface)
 *         → UniffiAppCore (JNI)
 *         → Rust aurora-mobile-ffi
 *         → aurora-bootstrap (SQLite + Tantivy + Crypto)
 */
public class MainActivity extends Activity {

    private WebView webView;
    private UniffiAppCore core;

    /** DK-08 §7.3：网络状态中继（ConnectivityManager → SyncGate） */
    private NetworkStateRelay networkRelay;
    /** P2P 同步引擎（懒启动，V19 §31 DEV-005） */
    private SyncEngine syncEngine;

    @SuppressLint({"SetJavaScriptEnabled", "AddJavascriptInterface"})
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        // 初始化 Rust 核心（真实 bootstrap: SQLite + Tantivy）
        String dataDir = getFilesDir().getAbsolutePath();
        try {
            core = new UniffiAppCore(dataDir);
        } catch (Throwable e) {
            Toast.makeText(this, "核心初始化失败: " + e.getMessage(), Toast.LENGTH_LONG).show();
            finish();
            return;
        }

        // DK-08 §7.3：网络状态接线（registerDefaultNetworkCallback → JNI 推送）
        networkRelay = new NetworkStateRelay(this);
        networkRelay.register();

        // WebView 容器
        webView = new WebView(this);
        webView.setLayoutParams(new FrameLayout.LayoutParams(
            FrameLayout.LayoutParams.MATCH_PARENT,
            FrameLayout.LayoutParams.MATCH_PARENT));

        WebSettings settings = webView.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setAllowFileAccess(false);
        settings.setAllowContentAccess(false);
        settings.setCacheMode(WebSettings.LOAD_DEFAULT);
        settings.setMediaPlaybackRequiresUserGesture(false);

        // JS bridge — React 侧通过 window.AndroidBridge 调用
        webView.addJavascriptInterface(new AndroidBridge(), "AndroidBridge");
        webView.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, String url) {
                // 只允许本地内容，阻止外部导航
                return !url.startsWith("file:///android_asset/");
            }
        });

        setContentView(webView);
        webView.loadUrl("file:///android_asset/index.html");
    }

    /**
     * JS Bridge — 暴露给 React 的原生接口。
     * 全部运行在 WebView JS 线程，JNI 调用本身是同步阻塞（毫秒级）。
     */
    private class AndroidBridge {

        @JavascriptInterface
        public String init(String dataDir) {
            return core != null ? "ok" : "error";
        }

        @JavascriptInterface
        public String listNotes() {
            try {
                List<UniffiAppCore.NoteSummary> notes = core.listNotes();
                JSONArray arr = new JSONArray();
                for (UniffiAppCore.NoteSummary n : notes) {
                    JSONObject o = new JSONObject();
                    o.put("id", n.id);
                    o.put("title", n.title);
                    o.put("updatedAt", n.updatedAt);
                    o.put("encryption", n.encryption);
                    arr.put(o);
                }
                return arr.toString();
            } catch (Exception e) {
                return "[]";
            }
        }

        @JavascriptInterface
        public boolean setNoteEncryption(String noteId, String level) {
            try {
                return core.setNoteEncryption(noteId, level);
            } catch (Exception e) {
                return false;
            }
        }

        /** DK-08 §7.3: 查询「仅 Wi-Fi 同步」（未初始化/异常恒 false = 不限制）。 */
        @JavascriptInterface
        public boolean getWifiOnly() {
            try {
                return core != null && core.getWifiOnly();
            } catch (Exception e) {
                return false;
            }
        }

        /** DK-08 §7.3: 设置「仅 Wi-Fi 同步」（KV 持久化 + 门即时切换）。 */
        @JavascriptInterface
        public void setWifiOnly(boolean on) {
            try {
                if (core != null) core.setWifiOnly(on);
            } catch (Exception e) {
                // 桥线程静默：失败值由下次 getWifiOnly 读取反映
            }
        }

        @JavascriptInterface
        public String todayFocusSummary() {
            try {
                return core.todayFocusSummary();
            } catch (Exception e) {
                return null;
            }
        }

        @JavascriptInterface
        public String createNote(String title) {
            try {
                return core.createNote(title);
            } catch (Exception e) {
                return null;
            }
        }

        @JavascriptInterface
        public String getNoteContent(String noteId) {
            try {
                return core.getNoteContent(noteId);
            } catch (Exception e) {
                return null;
            }
        }

        @JavascriptInterface
        public int saveNoteContent(String noteId, String content) {
            try {
                core.saveNoteContent(noteId, content);
                return 0;
            } catch (Exception e) {
                return -1;
            }
        }

        @JavascriptInterface
        public int deleteNote(String noteId) {
            try {
                core.deleteNote(noteId);
                return 0;
            } catch (Exception e) {
                return -1;
            }
        }

        @JavascriptInterface
        public String searchNotes(String query) {
            try {
                List<UniffiAppCore.SearchResult> results = core.searchNotes(query);
                JSONArray arr = new JSONArray();
                for (UniffiAppCore.SearchResult r : results) {
                    JSONObject o = new JSONObject();
                    o.put("noteId", r.noteId);
                    o.put("title", r.title);
                    o.put("snippet", r.snippet);
                    o.put("score", r.score);
                    arr.put(o);
                }
                return arr.toString();
            } catch (Exception e) {
                return "[]";
            }
        }

        @JavascriptInterface
        public boolean isFallback() {
            return core.isFallback();
        }

        // ------------------------------------------------------------------
        // DK-02 尾回收站 S2（JSON 串传递——searchNotes 惯例）
        // ------------------------------------------------------------------

        /** 回收站列表（deleted_at_ms 倒序）。JSON: [{noteId,title,deletedAtMs,originPath}] */
        @JavascriptInterface
        public String listTrashed() {
            try {
                List<UniffiAppCore.TrashedNoteItem> items = core.listTrashed();
                JSONArray arr = new JSONArray();
                for (UniffiAppCore.TrashedNoteItem t : items) {
                    JSONObject o = new JSONObject();
                    o.put("noteId", t.noteId);
                    o.put("title", t.title);
                    o.put("deletedAtMs", t.deletedAtMs);
                    o.put("originPath", t.originPath == null ? JSONObject.NULL : t.originPath);
                    arr.put(o);
                }
                return arr.toString();
            } catch (Exception e) {
                return "[]";
            }
        }

        /** 恢复笔记（软删标记移除）。 */
        @JavascriptInterface
        public boolean restoreNote(String noteId) {
            try {
                return core.restoreNote(noteId);
            } catch (Exception e) {
                return false;
            }
        }

        /** 彻底删除（仅回收站内可删——永久）。 */
        @JavascriptInterface
        public boolean purgeNote(String noteId) {
            try {
                return core.purgeNote(noteId);
            } catch (Exception e) {
                return false;
            }
        }

        /** 清理超期回收站（days 天前软删的永久删除）。返回清理数（-1=错误）。 */
        @JavascriptInterface
        public int purgeExpired(int days) {
            try {
                return core.purgeExpired(days).size();
            } catch (Exception e) {
                return -1;
            }
        }

        // ------------------------------------------------------------------
        // ProseMirror + Loro 编辑器（V19 §35 DEV-009）
        // ------------------------------------------------------------------

        /** 获取笔记 Loro 快照（base64）— 编辑器初始化。失败返回 null。 */
        @JavascriptInterface
        public String getNoteSnapshot(String noteId) {
            try {
                return core.getNoteSnapshot(noteId);
            } catch (Throwable e) {
                return null;
            }
        }


        /** 保存 Loro 快照（base64，CRDT 合并）+ 持久化。 */
        @JavascriptInterface
        public boolean saveNoteSnapshot(String noteId, String snapshotBase64) {
            try {
                return core.saveNoteSnapshot(noteId, snapshotBase64);
            } catch (Throwable e) {
                return false;
            }
        }

        /** 时间机器：列出笔记快照元数据（JSON 数组）。 */
        @JavascriptInterface
        public String listSnapshots(String noteId) {
            try {
                return core.listSnapshots(noteId);
            } catch (Throwable e) {
                return "[]";
            }
        }

        /** 时间机器：读取指定版本快照正文。 */
        @JavascriptInterface
        public String getSnapshotContent(String noteId, int version) {
            try {
                return core.getSnapshotContent(noteId, version);
            } catch (Throwable e) {
                return null;
            }
        }

        // ------------------------------------------------------------------
        // P2P 同步（V19 §31 DEV-005 — iroh QUIC + NAT 穿透）
        // ------------------------------------------------------------------

        /** DK-49：引擎惰性启动 + 生命周期挂接（drain 补发通道就绪）。 */
        private void ensureSyncEngine() {
            if (syncEngine == null) {
                syncEngine = SyncEngine.start();
            }
            if (syncEngine != null && core != null) {
                try {
                    core.attachSyncEngine(syncEngine);
                } catch (Throwable ignored) {
                }
            }
        }

        /** 启动同步引擎，返回本机端点地址 JSON（供对端 QR/输入交换）。 */
        @JavascriptInterface
        public String startSyncEngine() {
            try {
                ensureSyncEngine();
                return syncEngine != null ? syncEngine.localAddr() : null;
            } catch (Throwable e) {
                return null;
            }
        }

        /** 获取本机端点地址 JSON（未启动时启动）。 */
        @JavascriptInterface
        public String getSyncLocalAddr() {
            try {
                ensureSyncEngine();
                return syncEngine != null ? syncEngine.localAddr() : null;
            } catch (Throwable e) {
                return null;
            }
        }

        /**
         * 与对端同步指定笔记。
         * @param peerAddrJson 对端端点地址 JSON
         * @return 结果 JSON {success, sentBytes, receivedBytes, remotePeer, error}
         */
        @JavascriptInterface
        public String syncNote(String peerAddrJson, String noteId) {
            try {
                ensureSyncEngine();
                if (syncEngine == null) return "{\"success\":false,\"error\":\"engine start failed\"}";
                SyncEngine.SyncReport r = syncEngine.syncNote(core, peerAddrJson, noteId);
                JSONObject o = new JSONObject();
                o.put("success", r.success);
                o.put("sentBytes", r.sentBytes);
                o.put("receivedBytes", r.receivedBytes);
                o.put("remotePeer", r.remotePeer);
                o.put("error", r.error == null ? "" : r.error);
                return o.toString();
            } catch (Throwable e) {
                return "{\"success\":false,\"error\":\"" + e.getMessage() + "\"}";
            }
        }

        /** 启动指定笔记的接收循环（服务端角色，对端发起时自动合并+持久化）。 */
        @JavascriptInterface
        public boolean startAcceptSync(String noteId) {
            try {
                ensureSyncEngine();
                return syncEngine != null && syncEngine.startAcceptLoop(core, noteId);
            } catch (Throwable e) {
                return false;
            }
        }

        /** 关闭同步引擎。 */
        @JavascriptInterface
        public void closeSyncEngine() {
            if (syncEngine != null) {
                syncEngine.close();
                syncEngine = null;
            }
            // DK-49：引擎生命周期注销 — 防 drain 走已 close 的 transport
            try {
                if (core != null) core.detachSyncEngine();
            } catch (Throwable ignored) {
            }
        }

        // ------------------------------------------------------------------
        // DK-49：移动端同步数据面（对齐桌面 sync_commands 七函数 —
        // JSON 透传；错误降级 null/"[]"/error json，UI 侧按空态/提示处理）
        // ------------------------------------------------------------------

        /** 函数 1：同步状态聚合 JSON（gate/wifi_only/queue_len/pending_*）。 */
        @JavascriptInterface
        public String syncStatus() {
            try {
                return core.syncStatus();
            } catch (Throwable e) {
                return null;
            }
        }

        /** 函数 2：离线队列明细 JSON 数组。 */
        @JavascriptInterface
        public String queueList() {
            try {
                return core.queueList();
            } catch (Throwable e) {
                return "[]";
            }
        }

        /** 函数 3：手动补发 JSON；前置不满足返回 error json（不静默）。 */
        @JavascriptInterface
        public String queueDrain() {
            try {
                return core.queueDrain();
            } catch (Throwable e) {
                return "{\"error\":\"" + e.getMessage() + "\"}";
            }
        }

        /** 函数 4：待处理冲突合并视图 JSON 数组。 */
        @JavascriptInterface
        public String conflictList() {
            try {
                return core.conflictList();
            } catch (Throwable e) {
                return "[]";
            }
        }

        /** 函数 5：真冲突副本闭环；失败返回 false。 */
        @JavascriptInterface
        public boolean conflictResolve(String artifactId) {
            try {
                core.conflictResolve(artifactId);
                return true;
            } catch (Throwable e) {
                return false;
            }
        }

        /** 函数 6：语义冲突解决；失败返回 false。 */
        @JavascriptInterface
        public boolean conflictResolveSemantic(String conflictId, String resolution) {
            try {
                core.conflictResolveSemantic(conflictId, resolution);
                return true;
            } catch (Throwable e) {
                return false;
            }
        }
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();
        if (networkRelay != null) {
            networkRelay.unregister();
            networkRelay = null;
        }
        if (webView != null) {
            webView.destroy();
            webView = null;
        }
        if (core != null) {
            core.destroy();
            core = null;
        }
        if (syncEngine != null) {
            syncEngine.close();
            syncEngine = null;
        }
    }

    @Override
    public void onBackPressed() {
        if (webView != null && webView.canGoBack()) {
            webView.goBack();
        } else {
            super.onBackPressed();
        }
    }
}
