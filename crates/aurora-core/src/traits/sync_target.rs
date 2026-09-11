//! Trait: SyncTarget — 同步目标的抽象，支持 P2P、云端、局域网多种同步模式
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。
//! 回调注册方法 `watch` 保持同步签名（fire-and-forget）。
//!
//! V20 P0-3（GAP-03）对齐 §28.1 细粒度签名：
//! - 细粒度原语 `send_update` / `recv_update` / `sync_version`
//!   （增量同步语义；默认实现回退到全量 `sync`，传输适配器应覆写）
//! - [`SyncConfig`]（超时 / 重试 / 批量 / 压缩）与 `connect_with_config`
//! - [`ConnectionState`]（连接状态机，供 SyncRouter 健康度路由）
//!
//! 兼容策略：新增方法均带默认实现，现有 Iroh / WebSocket / Lan 三个
//! 适配器零改动通过编译；增量语义随各传输层逐步落地。

use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub url: String,
    pub protocol: SyncProtocol,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncProtocol {
    Iroh,
    WebSocket,
    Quic,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub id: String,
    pub endpoint: Endpoint,
}

#[derive(Debug, Clone)]
pub struct DocSet {
    pub doc_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SyncReport {
    pub sent_ops: usize,
    pub received_ops: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    Connected { conn_id: String },
    Disconnected { conn_id: String },
    Progress { progress: f32 },
    Error { message: String },
}

/// 连接状态机 — V20 §28.1（SyncRouter 按健康度选择链路的依据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// 未连接（初始态或显式断开后）。
    Disconnected,
    /// 连接建立中（NAT 穿透 / 握手阶段）。
    Connecting,
    /// 已连接，可进行同步。
    Connected,
    /// 连接失败（含降级判定：超过 `SyncConfig.max_retries`）。
    Failed,
}

/// 同步配置 — V20 §28.1。
///
/// 由 SyncRouter 按策略下发；`Default` 提供保守生产值。
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// 单次同步操作超时（毫秒）。
    pub timeout_ms: u64,
    /// 失败重试次数上限（超过后 ConnectionState → Failed，触发降级路由）。
    pub max_retries: u32,
    /// 批量同步的文档数上限（增量分批，避免单次过大）。
    pub batch_size: usize,
    /// 是否启用传输层压缩（oplog 已二进制，压缩收益视负载而定）。
    pub enable_compression: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 10_000,
            max_retries: 3,
            batch_size: 32,
            enable_compression: false,
        }
    }
}

/// 对端传输数据面（V23-I3 / T3 — sync 真搬运的最小契约）。
///
/// 生产实现: iroh-blobs / WebSocket 帧传输（aurora-sync::iroh_transport）；
/// 测试与 IST: [`InMemoryPeerTransport`]。`sync` 无传输时**大声失败**
/// —— 此前恒返零报告导致降级链永不触发（G-02 同型静默占位）。
#[async_trait]
pub trait PeerTransport: Send + Sync {
    /// 推送 oplog 到对端，返回对端确认接收的 op 数。
    async fn push(&self, doc_id: &str, ops: Vec<u8>) -> Result<usize, crate::Error>;
    /// 从对端拉取该文档当前增量字节（无增量返空 Vec — 非错误）。
    async fn pull(&self, doc_id: &str) -> Result<Vec<u8>, crate::Error>;
}

/// 内存对端（测试/IST — push 沉淀到对端存储，pull 返回对端存量）。
#[derive(Default)]
pub struct InMemoryPeerTransport {
    store: std::sync::Mutex<std::collections::BTreeMap<String, Vec<Vec<u8>>>>,
}

impl InMemoryPeerTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// 测试播种：对端已有该文档的增量。
    pub fn seed(&self, doc_id: &str, ops: Vec<u8>) {
        self.store
            .lock()
            .unwrap()
            .entry(doc_id.to_string())
            .or_default()
            .push(ops);
    }

    /// 测试断言：对端收到的某文档 op 总数。
    pub fn received_count(&self, doc_id: &str) -> usize {
        self.store
            .lock()
            .unwrap()
            .get(doc_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

#[async_trait]
impl PeerTransport for InMemoryPeerTransport {
    async fn push(&self, doc_id: &str, ops: Vec<u8>) -> Result<usize, crate::Error> {
        let mut store = self.store.lock().unwrap();
        store.entry(doc_id.to_string()).or_default().push(ops);
        Ok(1)
    }

    async fn pull(&self, doc_id: &str) -> Result<Vec<u8>, crate::Error> {
        // 语义: 取对端**未消费**的一条增量（FIFO）；空 = 无增量
        let mut store = self.store.lock().unwrap();
        Ok(store
            .get_mut(doc_id)
            .and_then(|v| (!v.is_empty()).then(|| v.remove(0)))
            .unwrap_or_default())
    }
}

/// 导出本地文档 oplog 字节（Loro ExportMode::Update 或等价）。
pub type OplogExportFn = Box<dyn Fn(&str) -> Result<Vec<u8>, crate::Error> + Send + Sync>;
/// 应用远端 oplog（Loro import 或等价）。
pub type OplogMergeFn = Box<dyn Fn(&str, Vec<u8>) -> Result<(), crate::Error> + Send + Sync>;

/// 同步数据源/汇钩子（sync 往返中导出本地 oplog、应用远端 oplog）。
pub struct SyncHooks {
    /// 导出本地文档 oplog 字节（Loro ExportMode::Update 或等价）。
    pub export: OplogExportFn,
    /// 应用远端 oplog（Loro import 或等价）。
    pub merge: OplogMergeFn,
}

/// 单文档增量更新载荷（CRDT oplog 字节，传输格式由适配器决定）。
#[derive(Debug, Clone)]
pub struct UpdatePayload {
    pub doc_id: String,
    /// Loro `ExportMode::Update` 字节（或传输适配器等价格式）。
    pub ops: Vec<u8>,
}

#[async_trait]
pub trait SyncTarget: Send + Sync {
    async fn connect(&mut self, endpoint: &Endpoint) -> Result<Connection, crate::Error>;

    /// 建立连接并应用配置（默认实现: 委托 `connect` + 忽略配置记录）。
    ///
    /// 传输适配器应覆写以落实超时 / 重试 / 压缩语义。
    async fn connect_with_config(
        &mut self,
        endpoint: &Endpoint,
        _config: &SyncConfig,
    ) -> Result<Connection, crate::Error> {
        self.connect(endpoint).await
    }

    async fn sync(&self, conn: &Connection, doc_set: &DocSet) -> Result<SyncReport, crate::Error>;

    /// 发送单文档增量更新（§28.1 细粒度原语）。
    ///
    /// 默认实现：回退全量 `sync`（语义保底 — 更新已包含在同步中），
    /// 传输适配器应覆写以提供真正的增量发送（省带宽、降延迟）。
    async fn send_update(
        &self,
        conn: &Connection,
        update: &UpdatePayload,
    ) -> Result<(), crate::Error> {
        let _ = update;
        self.sync(
            conn,
            &DocSet {
                doc_ids: vec![update.doc_id.clone()],
            },
        )
        .await
        .map(|_| ())
    }

    /// 接收单文档增量更新（§28.1 细粒度原语）。
    ///
    /// 默认实现：触发一次全量 `sync` 后返回空载荷（拉取语义已满足，
    /// 但无法给出精确字节）；适配器覆写后返回对端待传 oplog 字节。
    async fn recv_update(&self, conn: &Connection, doc_id: &str) -> Result<Vec<u8>, crate::Error> {
        let _ = doc_id;
        self.sync(
            conn,
            &DocSet {
                doc_ids: vec![doc_id.to_string()],
            },
        )
        .await
        .map(|_| Vec::new())
    }

    /// 查询对端文档版本（§28.1 细粒度原语 — 增量同步起点判定）。
    ///
    /// 默认实现：不支持版本协商，返回 `None`（调用方回退全量同步）。
    async fn sync_version(
        &self,
        _conn: &Connection,
        _doc_id: &str,
    ) -> Result<Option<u64>, crate::Error> {
        Ok(None)
    }

    /// 查询连接状态（SyncRouter 健康度路由依据）。
    ///
    /// 默认实现（V23-I0 / T1 修复）: **真实探测**而非恒真——
    /// 旧实现无条件返回 Connected，任何基于 state() 的降级判定
    /// 都恒真（G-02: 故障切换永远不会触发）。
    ///
    /// 新语义: 用 `sync_version` 轻量查询探测对端可达性——
    /// - 应答（含 None）→ `Connected`
    /// - 错误 → `Failed`
    /// - 超时（3s）→ `Failed`（不可达等价失败）
    ///
    /// 有状态追踪的实现体应覆盖此方法返回缓存状态，避免每次
    /// 探测的网络往返。
    async fn state(&self, conn: &Connection) -> Result<ConnectionState, crate::Error> {
        let probe = self.sync_version(conn, "__health_probe__");
        match tokio::time::timeout(std::time::Duration::from_secs(3), probe).await {
            Ok(Ok(_)) => Ok(ConnectionState::Connected),
            Ok(Err(_)) => Ok(ConnectionState::Failed),
            Err(_) => Ok(ConnectionState::Failed),
        }
    }

    /// 注册同步事件回调（fire-and-forget，保持同步签名）。
    fn watch(&self, callback: Box<dyn Fn(SyncEvent) + Send + Sync>);
    async fn disconnect(&self, conn: &Connection) -> Result<(), crate::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// 最小可编译实现 — 验证默认实现链路（send/recv/version 回退 sync）。
    struct EchoTarget {
        sync_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl SyncTarget for EchoTarget {
        async fn connect(&mut self, _endpoint: &Endpoint) -> Result<Connection, crate::Error> {
            Ok(Connection {
                id: "c1".into(),
                endpoint: Endpoint {
                    url: "echo://local".into(),
                    protocol: SyncProtocol::WebSocket,
                },
            })
        }

        async fn sync(
            &self,
            _conn: &Connection,
            doc_set: &DocSet,
        ) -> Result<SyncReport, crate::Error> {
            self.sync_calls.fetch_add(1, Ordering::SeqCst);
            Ok(SyncReport {
                sent_ops: doc_set.doc_ids.len(),
                received_ops: 0,
                duration_ms: 0,
            })
        }

        fn watch(&self, _callback: Box<dyn Fn(SyncEvent) + Send + Sync>) {}

        async fn disconnect(&self, _conn: &Connection) -> Result<(), crate::Error> {
            Ok(())
        }
    }

    // ── T1（V23-I0）: state() 三态判定不恒真 ──

    /// 应答探测的目标（模拟健康对端）。
    struct HealthyTarget;

    #[async_trait]
    impl SyncTarget for HealthyTarget {
        async fn connect(&mut self, _endpoint: &Endpoint) -> Result<Connection, crate::Error> {
            Ok(Connection {
                id: "h".into(),
                endpoint: Endpoint {
                    url: "healthy://x".into(),
                    protocol: SyncProtocol::Quic,
                },
            })
        }
        async fn sync(
            &self,
            _conn: &Connection,
            doc_set: &DocSet,
        ) -> Result<SyncReport, crate::Error> {
            Ok(SyncReport {
                sent_ops: doc_set.doc_ids.len(),
                received_ops: 0,
                duration_ms: 0,
            })
        }
        fn watch(&self, _cb: Box<dyn Fn(SyncEvent) + Send + Sync>) {}
        async fn disconnect(&self, _conn: &Connection) -> Result<(), crate::Error> {
            Ok(())
        }
    }

    /// 探测必错的目标（模拟故障对端）。
    struct BrokenTarget;

    #[async_trait]
    impl SyncTarget for BrokenTarget {
        async fn connect(&mut self, _endpoint: &Endpoint) -> Result<Connection, crate::Error> {
            Ok(Connection {
                id: "b".into(),
                endpoint: Endpoint {
                    url: "broken://x".into(),
                    protocol: SyncProtocol::Quic,
                },
            })
        }
        async fn sync(
            &self,
            _conn: &Connection,
            doc_set: &DocSet,
        ) -> Result<SyncReport, crate::Error> {
            Ok(SyncReport {
                sent_ops: doc_set.doc_ids.len(),
                received_ops: 0,
                duration_ms: 0,
            })
        }
        async fn sync_version(
            &self,
            _conn: &Connection,
            _doc_id: &str,
        ) -> Result<Option<u64>, crate::Error> {
            Err(crate::Error::Internal("peer unreachable".into()))
        }
        fn watch(&self, _cb: Box<dyn Fn(SyncEvent) + Send + Sync>) {}
        async fn disconnect(&self, _conn: &Connection) -> Result<(), crate::Error> {
            Ok(())
        }
    }

    /// 探测挂起的目标（模拟不可达 — 应超时判 Failed）。
    struct HangingTarget;

    #[async_trait]
    impl SyncTarget for HangingTarget {
        async fn connect(&mut self, _endpoint: &Endpoint) -> Result<Connection, crate::Error> {
            Ok(Connection {
                id: "g".into(),
                endpoint: Endpoint {
                    url: "hang://x".into(),
                    protocol: SyncProtocol::Quic,
                },
            })
        }
        async fn sync(
            &self,
            _conn: &Connection,
            doc_set: &DocSet,
        ) -> Result<SyncReport, crate::Error> {
            Ok(SyncReport {
                sent_ops: doc_set.doc_ids.len(),
                received_ops: 0,
                duration_ms: 0,
            })
        }
        async fn sync_version(
            &self,
            _conn: &Connection,
            _doc_id: &str,
        ) -> Result<Option<u64>, crate::Error> {
            // 挂起 60s — 超过 3s 探测上限（tokio paused 时间自动推进）
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            Ok(None)
        }
        fn watch(&self, _cb: Box<dyn Fn(SyncEvent) + Send + Sync>) {}
        async fn disconnect(&self, _conn: &Connection) -> Result<(), crate::Error> {
            Ok(())
        }
    }

    /// T1 验收: 默认 state() 三态判定——健康 Connected / 故障 Failed /
    /// 不可达超时 Failed。修复前恒返 Connected。
    #[tokio::test(start_paused = true)]
    async fn t1_state_probe_not_always_connected() {
        // 1) 健康对端（默认 sync_version 返 Ok(None)）→ Connected
        let mut t = HealthyTarget;
        let conn = t
            .connect(&Endpoint {
                url: "h://x".into(),
                protocol: SyncProtocol::Quic,
            })
            .await
            .unwrap();
        assert!(matches!(
            t.state(&conn).await.unwrap(),
            ConnectionState::Connected
        ));

        // 2) 故障对端（sync_version 返 Err）→ Failed
        let mut b = BrokenTarget;
        let bconn = b
            .connect(&Endpoint {
                url: "b://x".into(),
                protocol: SyncProtocol::Quic,
            })
            .await
            .unwrap();
        assert!(matches!(
            b.state(&bconn).await.unwrap(),
            ConnectionState::Failed
        ));

        // 3) 挂起对端（超时）→ Failed（paused 时钟自动推进过 3s 窗口）
        let mut g = HangingTarget;
        let gconn = g
            .connect(&Endpoint {
                url: "g://x".into(),
                protocol: SyncProtocol::Quic,
            })
            .await
            .unwrap();
        assert!(matches!(
            g.state(&gconn).await.unwrap(),
            ConnectionState::Failed
        ));
    }

    #[tokio::test]
    async fn fine_grained_primitives_fall_back_to_sync() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut target = EchoTarget {
            sync_calls: calls.clone(),
        };
        let conn = target
            .connect(&Endpoint {
                url: "echo://x".into(),
                protocol: SyncProtocol::Quic,
            })
            .await
            .unwrap();

        // send_update → 默认走 sync
        target
            .send_update(
                &conn,
                &UpdatePayload {
                    doc_id: "note-1".into(),
                    ops: vec![1, 2, 3],
                },
            )
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // recv_update → 默认走 sync
        let got = target.recv_update(&conn, "note-1").await.unwrap();
        assert!(got.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        // sync_version → 默认 None（调用方回退全量）
        assert!(target
            .sync_version(&conn, "note-1")
            .await
            .unwrap()
            .is_none());

        // state → 默认 Connected
        assert_eq!(
            target.state(&conn).await.unwrap(),
            ConnectionState::Connected
        );

        // connect_with_config → 默认委托 connect
        let c2 = target
            .connect_with_config(
                &Endpoint {
                    url: "echo://y".into(),
                    protocol: SyncProtocol::Iroh,
                },
                &SyncConfig::default(),
            )
            .await
            .unwrap();
        assert_eq!(c2.id, "c1");
    }

    #[test]
    fn sync_config_defaults_are_conservative() {
        let c = SyncConfig::default();
        assert_eq!(c.timeout_ms, 10_000);
        assert_eq!(c.max_retries, 3);
        assert_eq!(c.batch_size, 32);
        assert!(!c.enable_compression);
    }
}
