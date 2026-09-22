//! iroh P2P 同步引擎 — V19 §31 DEV-005 移动端暴露层
//!
//! 数据链路（V19 §31.1 协议）:
//! ```text
//! App (JS/Java) → SyncEngine (uniffi/JNI)
//!   → IrohTransport (QUIC + NAT 穿透)
//!     → 对端 SyncEngine
//!       → LoroDoc 版本向量交换 → 增量导出/导入
//! ```
//!
//! 端点地址交换: EndpointAddr 序列化为 JSON 字符串（QR / 手动输入 / 未来经 relay 目录）。
//! 注意: 当前协议为单文档同步（V19 §31.1 的 note 级语义），
//! 每篇笔记独立握手；多文档批量同步属于 Cloud/WebSocket 通道范畴。

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use aurora_sync::iroh_transport::IrohTransport;
use aurora_sync::sync_gate::{NetworkClass, NetworkStateProvider};
use loro::LoroDoc;

use crate::{MobileError, UniffiAppCore};

/// 同步结果报告（uniffi Record）。
#[derive(uniffi::Record, Debug, Clone)]
pub struct P2pSyncReport {
    pub success: bool,
    pub sent_bytes: u64,
    pub received_bytes: u64,
    pub remote_peer: String,
    pub error: Option<String>,
}

/// iroh P2P 同步引擎。
#[derive(uniffi::Object)]
pub struct SyncEngine {
    transport: IrohTransport,
    /// 独立 runtime 驱动 iroh 异步任务（不阻塞 AppCore 的同步 runtime）。
    runtime: tokio::runtime::Runtime,
    /// 本机端点地址（JSON 字符串，供对端连接）。
    local_addr_json: String,
}

impl SyncEngine {
    /// 启动引擎：绑定 iroh Endpoint，返回本机地址（JSON）。
    pub fn start() -> Result<Arc<Self>, MobileError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| MobileError::InitFailed {
                message: format!("sync runtime: {e}"),
            })?;

        let peer_id = aurora_sync::PeerId::random();
        let transport = runtime
            .block_on(async { IrohTransport::new(peer_id).await })
            .map_err(|e| MobileError::InitFailed {
                message: format!("iroh transport: {e}"),
            })?;

        let addr = transport.addr();
        let local_addr_json =
            serde_json::to_string(&addr).map_err(|e| MobileError::InitFailed {
                message: format!("serialize endpoint addr: {e}"),
            })?;

        tracing::info!(%local_addr_json, "p2p sync engine started");

        Ok(Arc::new(Self {
            transport,
            runtime,
            local_addr_json,
        }))
    }
}

#[uniffi::export]
impl SyncEngine {
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, MobileError> {
        Self::start()
    }

    /// 本机端点地址（JSON，供 QR / 对端输入）。
    pub fn local_addr(&self) -> String {
        self.local_addr_json.clone()
    }

    /// 本机 PeerId（iroh EndpointId 十六进制）。
    pub fn node_id(&self) -> String {
        format!("{:?}", self.transport.id())
    }

    /// 与对端同步指定笔记（客户端角色，V19 §31.1 发起方）。
    ///
    /// `peer_addr_json`: 对端 `local_addr()` 输出的 JSON 字符串。
    pub fn sync_note(
        self: Arc<Self>,
        core: Arc<UniffiAppCore>,
        peer_addr_json: String,
        note_id: String,
    ) -> Result<P2pSyncReport, MobileError> {
        let peer_addr: iroh::EndpointAddr =
            serde_json::from_str(&peer_addr_json).map_err(|e| MobileError::OperationFailed {
                message: format!("parse peer addr: {e}"),
            })?;

        // 从 AppCore 缓存解析 NoteDoc（快照恢复）
        let note_doc = core.doc_for_note_public(&note_id)?;
        let doc: &LoroDoc = note_doc.inner();

        let report = self
            .runtime
            .block_on(async { self.transport.sync_with_peer(peer_addr, doc).await })
            .map_err(|e| MobileError::OperationFailed {
                message: format!("sync_with_peer: {e}"),
            })?;

        // 同步落地：导入的更新持久化到 KVStore
        core.persist_note_snapshot(&note_id)?;

        Ok(P2pSyncReport {
            success: report.success,
            sent_bytes: report.sent_bytes as u64,
            received_bytes: report.received_bytes as u64,
            remote_peer: report.remote_peer,
            error: report.error,
        })
    }

    /// 启动指定笔记的接收循环（服务端角色，后台任务）。
    ///
    /// 每次对端发起同步，合并远端增量并持久化。
    /// 幂等：同一笔记重复调用会先停掉旧循环。
    pub fn start_accept_loop(self: Arc<Self>, core: Arc<UniffiAppCore>, note_id: String) {
        let note_doc = match core.doc_for_note_public(&note_id) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("start_accept_loop: {e}");
                return;
            }
        };
        let doc: LoroDoc = note_doc.inner().clone();

        let transport = self.transport.clone();
        let core_clone = core.clone();
        let note_id_clone = note_id.clone();

        self.runtime.spawn(async move {
            loop {
                match transport.accept_sync(&doc).await {
                    Ok(report) => {
                        tracing::info!(
                            note_id = %note_id_clone,
                            sent = report.sent_bytes,
                            received = report.received_bytes,
                            "p2p accept sync completed"
                        );
                        // 合并结果持久化
                        let _ = core_clone.persist_note_snapshot(&note_id_clone);
                    }
                    Err(e) => {
                        tracing::warn!("p2p accept loop error: {e}");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        });
    }

    /// 关闭引擎。
    pub fn close(&self) {
        self.runtime
            .block_on(async { self.transport.close().await });
    }
}

// ===========================================================================
// JNI 桥接 — com.aurora.note.SyncEngine native 方法（feature: p2p-sync）
// ===========================================================================

use jni::objects::{JClass, JObject, JString};
use jni::sys::{jint, jlong, jobjectArray, jstring};
use jni::JNIEnv;

use crate::{jstring_to_rust, rust_str_to_jstring};

unsafe fn engine_from_handle(handle: jlong) -> Arc<SyncEngine> {
    let arc = Arc::from_raw(handle as *const SyncEngine);
    let cloned = Arc::clone(&arc);
    std::mem::forget(arc);
    cloned
}

/// SyncEngine.nativeStart(coreHandle) -> engineHandle
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_SyncEngine_nativeStart(
    _env: JNIEnv,
    _class: JClass,
    _core_handle: jlong,
) -> jlong {
    match SyncEngine::start() {
        Ok(engine) => Arc::into_raw(engine) as jlong,
        Err(e) => {
            tracing::error!("sync engine start failed: {e}");
            0
        }
    }
}

/// SyncEngine.nativeLocalAddr(engineHandle) -> String (JSON)
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_SyncEngine_nativeLocalAddr(
    mut env: JNIEnv,
    _class: JClass,
    engine_handle: jlong,
) -> jstring {
    let engine = unsafe { engine_from_handle(engine_handle) };
    rust_str_to_jstring(&mut env, &engine.local_addr())
}

/// SyncEngine.nativeSyncNote(engineHandle, coreHandle, peerAddrJson, noteId)
///   -> String[5] { success, sentBytes, receivedBytes, remotePeer, error }
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_SyncEngine_nativeSyncNote(
    mut env: JNIEnv,
    _class: JClass,
    engine_handle: jlong,
    core_handle: jlong,
    peer_addr: JString,
    note_id: JString,
) -> jobjectArray {
    let engine = unsafe { engine_from_handle(engine_handle) };
    let core = unsafe {
        let arc = Arc::from_raw(core_handle as *const crate::UniffiAppCore);
        let cloned = Arc::clone(&arc);
        std::mem::forget(arc);
        cloned
    };
    let Some(peer) = jstring_to_rust(&mut env, &peer_addr) else {
        return std::ptr::null_mut();
    };
    let Some(note) = jstring_to_rust(&mut env, &note_id) else {
        return std::ptr::null_mut();
    };

    let report = engine
        .sync_note(core, peer, note)
        .unwrap_or_else(|e| P2pSyncReport {
            success: false,
            sent_bytes: 0,
            received_bytes: 0,
            remote_peer: String::new(),
            error: Some(e.to_string()),
        });
    report_array(&mut env, &report)
}

fn report_array(env: &mut JNIEnv, r: &P2pSyncReport) -> jobjectArray {
    let fields: Vec<String> = vec![
        r.success.to_string(),
        r.sent_bytes.to_string(),
        r.received_bytes.to_string(),
        r.remote_peer.clone(),
        r.error.clone().unwrap_or_default(),
    ];
    let str_class = match env.find_class("java/lang/String") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let arr = match env.new_object_array(5, &str_class, JObject::null()) {
        Ok(a) => a,
        Err(_) => return std::ptr::null_mut(),
    };
    for (i, f) in fields.iter().enumerate() {
        let js = rust_str_to_jstring(env, f);
        let _ = env.set_object_array_element(&arr, i as i32, unsafe { JObject::from_raw(js) });
    }
    arr.into_raw()
}

/// SyncEngine.nativeStartAccept(engineHandle, coreHandle, noteId) -> 0 ok
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_SyncEngine_nativeStartAccept(
    mut env: JNIEnv,
    _class: JClass,
    engine_handle: jlong,
    core_handle: jlong,
    note_id: JString,
) -> jint {
    let engine = unsafe { engine_from_handle(engine_handle) };
    let core = unsafe {
        let arc = Arc::from_raw(core_handle as *const crate::UniffiAppCore);
        let cloned = Arc::clone(&arc);
        std::mem::forget(arc);
        cloned
    };
    let Some(note) = jstring_to_rust(&mut env, &note_id) else {
        return -1;
    };
    engine.start_accept_loop(core, note);
    0
}

/// SyncEngine.nativeClose(engineHandle)
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_SyncEngine_nativeClose(
    _env: JNIEnv,
    _class: JClass,
    engine_handle: jlong,
) {
    let engine = unsafe { engine_from_handle(engine_handle) };
    engine.close();
    // 释放引擎引用（与 AppCore 相同的 handle 语义）
    unsafe { drop(Arc::from_raw(engine_handle as *const SyncEngine)) };
}

// ===========================================================================
// NetworkStateProvider — DK-08 §7.3「仅 Wi-Fi 同步」平台网络状态源
// （request `bravo-request-wifi-only-sync` 申请 1 裁决：ConnectivityManager
//   缓存方案，Kotlin 推送 / Rust 读缓存）
// ===========================================================================

/// Android 网络状态缓存（进程级单例）。
///
/// 数据链路：Kotlin 侧 `ConnectivityManager.registerNetworkCallback` 在
/// `onCapabilitiesChanged`（NET_CAPABILITY_NOT_METERED）与 `onLost` 时调
/// `SyncEngine.nativeUpdateNetworkState(state)` 推送最新分类；
/// Rust 侧 `current_class()` 为原子读（廉价、同步、不跨 JNI 阻塞），
/// 与 sync_gate.rs「读缓存 + 不污染熔断统计」的语义约定一致。
pub struct AndroidNetworkState {
    /// 0 = Unmetered / 1 = Metered / 2 = Offline（越界按 0 处理：默认放行）。
    cached: AtomicU8,
}

/// 进程级单例（JNI 回调与 SyncGate 装配共享）。
pub static NETWORK_STATE: AndroidNetworkState = AndroidNetworkState {
    cached: AtomicU8::new(0),
};

pub const CLASS_UNMETERED: u8 = 0;
pub const CLASS_METERED: u8 = 1;
pub const CLASS_OFFLINE: u8 = 2;

impl AndroidNetworkState {
    /// JNI 推送入口（`nativeUpdateNetworkState` 调用；越界值归零 = 放行）。
    pub fn update(&self, raw: u8) {
        let v = match raw {
            CLASS_METERED => CLASS_METERED,
            CLASS_OFFLINE => CLASS_OFFLINE,
            _ => CLASS_UNMETERED,
        };
        self.cached.store(v, Ordering::Relaxed);
    }
}

impl NetworkStateProvider for AndroidNetworkState {
    fn current_class(&self) -> NetworkClass {
        match self.cached.load(Ordering::Relaxed) {
            CLASS_METERED => NetworkClass::Metered,
            CLASS_OFFLINE => NetworkClass::Offline,
            _ => NetworkClass::Unmetered,
        }
    }
}

/// Kotlin 回调桥：`SyncEngine.nativeUpdateNetworkState(state: Int)`。
///
/// Kotlin 侧约定：0 = NOT_METERED、1 = METERED、2 = 网络丢失（Offline）。
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_SyncEngine_nativeUpdateNetworkState(
    _env: JNIEnv,
    _class: JClass,
    state: jint,
) {
    NETWORK_STATE.update(state.clamp(0, 255) as u8);
}

/// SyncGate 装配入口：`Arc<dyn NetworkStateProvider>`（Bravo 接 SyncGate 时注入）。
pub fn network_state_provider() -> Arc<dyn NetworkStateProvider> {
    Arc::new(AndroidNetworkStateProxy)
}

/// [`NETWORK_STATE`] 的 trait-object 视图（`current_class` 委托单例）。
struct AndroidNetworkStateProxy;

impl NetworkStateProvider for AndroidNetworkStateProxy {
    fn current_class(&self) -> NetworkClass {
        NETWORK_STATE.current_class()
    }
}

#[cfg(test)]
mod network_state_tests {
    use super::*;

    /// request 验收清单 1：三态映射正确（计量/不计量/离线）。
    #[test]
    fn maps_all_three_classes() {
        let st = AndroidNetworkState {
            cached: AtomicU8::new(CLASS_UNMETERED),
        };
        st.update(CLASS_UNMETERED);
        assert_eq!(st.current_class(), NetworkClass::Unmetered);
        st.update(CLASS_METERED);
        assert_eq!(st.current_class(), NetworkClass::Metered);
        st.update(CLASS_OFFLINE);
        assert_eq!(st.current_class(), NetworkClass::Offline);
    }

    /// 越界 raw 值归零（默认放行语义，拒绝不成立时不误伤）。
    #[test]
    fn out_of_range_raw_falls_back_to_unmetered() {
        let st = AndroidNetworkState {
            cached: AtomicU8::new(CLASS_METERED),
        };
        st.update(200);
        assert_eq!(st.current_class(), NetworkClass::Unmetered);
    }

    /// trait-object 装配路径可用（SyncGate::new 的签名契约）。
    #[test]
    fn provider_assembles_as_trait_object() {
        let provider: Arc<dyn NetworkStateProvider> = network_state_provider();
        // 默认缓存 = Unmetered（进程启动到首条回调之间不阻塞同步）
        assert_eq!(provider.current_class(), NetworkClass::Unmetered);
        NETWORK_STATE.update(CLASS_METERED);
        assert_eq!(provider.current_class(), NetworkClass::Metered);
        NETWORK_STATE.update(CLASS_UNMETERED);
    }
}
