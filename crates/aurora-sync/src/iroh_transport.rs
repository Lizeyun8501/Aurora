//! iroh 真实传输层 (iroh Real Transport) — V19 §31 DEV-005
//!
//! 基于 iroh 1.0 的 QUIC 传输层，实现 Loro CRDT 更新的 P2P 同步。
//!
//! # 架构
//! ```text
//! ┌──────────────┐                          ┌──────────────┐
//! │ Device A     │                          │ Device B     │
//! │ LoroDoc      │◄──── QUIC Stream ───────►│ LoroDoc      │
//! │ (peer=1)     │    iroh Endpoint         │ (peer=2)     │
//! └──────────────┘                          └──────────────┘
//! ```
//!
//! # 同步流程（V19 §31.1）
//! 1. Device A 通过 iroh Endpoint 连接 Device B (按 EndpointId 拨号)
//! 2. 建立 QUIC 双向流 (open_bi)
//! 3. 交换版本向量: A发送 doc.oplog_vv() → B
//! 4. 差异计算: B 比较 VV，导出增量 update
//! 5. 传输增量: B → A 发送 update bytes
//! 6. Import & Apply: A 接收并 import 到 LoroDoc
//! 7. 反向同步: A → B（新双向流）
//!
//! 本模块提供 [`IrohTransport`] 作为 [`crate::p2p::MockTransport`] 的生产替代。

use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

// iroh 1.0 API（对应 V19 §31.2）
use iroh::endpoint::presets::Minimal;
use iroh::{Endpoint, EndpointAddr};
use loro::LoroDoc;

use crate::p2p::PeerId;

use std::time::Duration;

/// 连接滞留窗口：iroh/noq 在最后一个 `Connection` 句柄 drop 时立即
/// 应用层关闭（ApplicationClose 0），会作废在途流数据/FIN。收尾侧
/// 克隆句柄滞留该窗口，保证对端在连接存活期内读完收尾数据。
/// （内存网络 RTT ~1ms，生产真实网络 1s 足够覆盖正常 RTT 抖动）
const CONN_LINGER: Duration = Duration::from_secs(1);

/// 克隆连接句柄并滞留 CONN_LINGER 后释放（收尾保护）。
fn linger_conn(conn: &iroh::endpoint::Connection) {
    let held = conn.clone();
    tokio::spawn(async move {
        tokio::time::sleep(CONN_LINGER).await;
        drop(held);
    });
}

/// iroh ALPN 协议标识（V19 §31.2: `aurora-note/1`）。
pub const AURORA_ALPN: &[u8] = b"aurora-note/1";

/// 同步消息最大大小（10 MB，对应 V19 §31.2 `read_to_end` 限制）。
pub const MAX_SYNC_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

/// 版本向量编码帧：长度前缀 (4 bytes BE) + bincode 序列化数据。
fn encode_frame(data: &[u8]) -> Vec<u8> {
    let len = data.len() as u32;
    let mut buf = len.to_be_bytes().to_vec();
    buf.extend_from_slice(data);
    buf
}

/// 解析帧：返回 payload（不含长度前缀），返回剩余偏移。
#[cfg(test)]
fn decode_frame(buf: &[u8]) -> Option<(&[u8], &[u8])> {
    if buf.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + len {
        return None;
    }
    Some((&buf[4..4 + len], &buf[4 + len..]))
}

/// iroh 真实传输层 — 封装 iroh Endpoint，提供 QUIC 双向流同步。
///
/// 对应 V19 §31.2 `IrohSyncTarget`，使用 iroh 1.0 API。
/// `Clone`: Endpoint 内部为 Arc 句柄，克隆廉价，供多笔记并发同步。
pub struct IrohTransport {
    /// iroh Endpoint（QUIC 监听器 + NAT 穿透）。
    endpoint: Endpoint,
    /// 本节点的 PeerId（映射到 iroh NodeId）。
    peer_id: Mutex<PeerId>,
}

impl Clone for IrohTransport {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            peer_id: Mutex::new(self.peer_id()),
        }
    }
}

/// 同步结果报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    /// 发送的字节数。
    pub sent_bytes: usize,
    /// 接收的字节数。
    pub received_bytes: usize,
    /// 远端 PeerId。
    pub remote_peer: String,
    /// 是否成功。
    pub success: bool,
    /// 错误信息（如有）。
    pub error: Option<String>,
}

impl IrohTransport {
    /// 创建 iroh Endpoint 并绑定（V19 §31.2 `IrohSyncTarget::new`）。
    ///
    /// 使用 `Minimal` 预设（无 relay/发现的纯直连端点），自动完成基本绑定。
    pub async fn new(peer_id: PeerId) -> Result<Self, String> {
        // Minimal 预设 = Empty + rustls ring provider（tls-ring 默认 feature）。
        // Empty 预设不设 crypto provider，bind 必失败（iroh 1.0.3 语义）。
        let endpoint = Endpoint::builder(Minimal)
            .alpns(vec![AURORA_ALPN.to_vec()])
            .bind()
            .await
            .map_err(|e| {
                error!("iroh Endpoint bind failed: {}", e);
                format!("iroh bind failed: {}", e)
            })?;

        info!("iroh transport bound: peer_id={}", peer_id,);

        Ok(Self {
            endpoint,
            peer_id: Mutex::new(peer_id),
        })
    }

    /// 返回本节点的 iroh EndpointId。
    pub fn id(&self) -> iroh::EndpointId {
        self.endpoint.id()
    }

    /// 返回本节点的 PeerId。
    pub fn peer_id(&self) -> PeerId {
        self.peer_id.lock().clone()
    }

    /// 返回 iroh EndpointAddr（含 relay 地址与直连地址），可分享给对端用于连接。
    pub fn addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    /// 发起同步（客户端角色）：连接对端并交换 Loro CRDT 增量。
    ///
    /// V19 §31.2 `sync_with_peer` 流程：
    /// 1. 建立 QUIC 连接
    /// 2. open_bi 创建双向流
    /// 3. 发送本地版本向量
    /// 4. 接收远端增量更新
    /// 5. import 到本地 LoroDoc
    /// 6. 反向：发送本地缺失更新
    pub async fn sync_with_peer(
        &self,
        peer_addr: EndpointAddr,
        local_doc: &LoroDoc,
    ) -> Result<SyncReport, String> {
        debug!("sync_with_peer: connecting to node_id={:?}", peer_addr.id);

        // 1. 建立 QUIC 连接
        let conn = self
            .endpoint
            .connect(peer_addr.clone(), AURORA_ALPN)
            .await
            .map_err(|e| format!("iroh connect failed: {}", e))?;

        // 2. 创建双向流 — 发送版本向量
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| format!("open_bi failed: {}", e))?;

        // 3. 发送本地版本向量（Loro oplog version）
        let local_vv = local_doc.oplog_vv();
        let vv_bytes = local_vv.encode();
        let frame = encode_frame(&vv_bytes);
        send.write_all(&frame)
            .await
            .map_err(|e| format!("write vv failed: {}", e))?;
        send.finish()
            .map_err(|e| format!("finish send failed: {}", e))?;

        // 4. 接收远端增量更新
        let remote_update = recv
            .read_to_end(MAX_SYNC_MESSAGE_SIZE)
            .await
            .map_err(|e| format!("read update failed: {}", e))?;

        let received_bytes = remote_update.len();

        // 5. Import 到本地 LoroDoc
        if !remote_update.is_empty() {
            local_doc
                .import(&remote_update)
                .map_err(|e| format!("loro import failed: {}", e))?;
            debug!("imported {} bytes from peer", received_bytes);
        }

        // 6. 反向同步：服务端主动发起 stream2（其 open_bi 后立即写 VV，
        //    保证客户端 accept_bi 时数据已在途；若由客户端先开流再等
        //    服务端写，QUIC 首帧未带数据时服务端 accept_bi 永远不可见，
        //    双方互等死锁 — V19 协议缺陷，本切片修复）。
        //    VV 采用长度前缀帧读取（服务端此时不能 FIN，否则客户端无法
        //    区分 VV 边界；服务端 FIN 延后到 import 完成作为完成信号）。
        let (mut send2, mut recv2) = conn
            .accept_bi()
            .await
            .map_err(|e| format!("accept_bi reverse failed: {}", e))?;

        // 接收远端版本向量（长度前缀帧，无 FIN）
        let mut len_buf = [0u8; 4];
        recv2
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| format!("read vv frame len failed: {}", e))?;
        let vv_len = u32::from_be_bytes(len_buf) as usize;
        let mut vv_buf = vec![0u8; vv_len];
        recv2
            .read_exact(&mut vv_buf)
            .await
            .map_err(|e| format!("read vv frame body failed: {}", e))?;

        let remote_vv = loro::VersionVector::decode(&vv_buf)
            .map_err(|e| format!("decode remote vv failed: {}", e))?;

        // 导出本地相对远端的增量
        let local_update = local_doc
            .export(loro::ExportMode::updates(&remote_vv))
            .map_err(|e| format!("loro export failed: {}", e))?;

        let sent_bytes = local_update.len();
        send2
            .write_all(&local_update)
            .await
            .map_err(|e| format!("write reverse update failed: {}", e))?;
        send2
            .finish()
            .map_err(|e| format!("finish reverse send failed: {}", e))?;

        // 7. 等待服务端完成信号：服务端 import 完成后才 FIN stream2，
        //    客户端收到 EOF 再返回——确保服务端先收尾，避免连接句柄
        //    drop 触发的自动关闭抢先于在途数据（noq 语义）。
        recv2
            .read_to_end(1)
            .await
            .map_err(|e| format!("wait server completion failed: {}", e))?;
        linger_conn(&conn);

        info!(
            "sync_with_peer completed: sent={} bytes, received={} bytes",
            sent_bytes, received_bytes
        );

        Ok(SyncReport {
            sent_bytes,
            received_bytes,
            remote_peer: format!("{:?}", peer_addr.id),
            success: true,
            error: None,
        })
    }

    /// 接收同步（服务端角色）：监听入站连接并响应。
    ///
    /// 在 tokio 任务中循环调用此方法以持续接收同步请求。
    pub async fn accept_sync(&self, local_doc: &LoroDoc) -> Result<SyncReport, String> {
        debug!("accept_sync: waiting for incoming connection...");

        // 接受入站连接
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| "endpoint closed".to_string())?;

        let conn = incoming
            .accept()
            .map_err(|e| format!("accept connection failed: {}", e))?
            .await
            .map_err(|e| format!("accept connection error: {}", e))?;

        // 接收双向流
        let (mut send, mut recv) = conn
            .accept_bi()
            .await
            .map_err(|e| format!("accept_bi failed: {}", e))?;

        // 读取远端版本向量
        let remote_vv_data = recv
            .read_to_end(1024)
            .await
            .map_err(|e| format!("read remote vv failed: {}", e))?;

        let remote_vv = loro::VersionVector::decode(&remote_vv_data)
            .map_err(|e| format!("decode remote vv failed: {}", e))?;

        // 导出本地相对远端的增量
        let local_update = local_doc
            .export(loro::ExportMode::updates(&remote_vv))
            .map_err(|e| format!("loro export failed: {}", e))?;

        let sent_bytes = local_update.len();
        send.write_all(&local_update)
            .await
            .map_err(|e| format!("write update failed: {}", e))?;
        send.finish()
            .map_err(|e| format!("finish send failed: {}", e))?;

        // 反向：服务端主动开流，推送本地版本向量（不 FIN——FIN 延后到
        // import 完成后作为完成信号），再接收客户端缺失更新
        let (mut send2, mut recv2) = conn
            .open_bi()
            .await
            .map_err(|e| format!("open_bi reverse failed: {}", e))?;

        // 发送本地版本向量（长度前缀帧）
        let local_vv = local_doc.oplog_vv();
        let vv_bytes = local_vv.encode();
        let frame = encode_frame(&vv_bytes);
        send2
            .write_all(&frame)
            .await
            .map_err(|e| format!("write vv reverse failed: {}", e))?;

        // 接收远端增量（客户端 FIN → EOF）
        let remote_update = recv2
            .read_to_end(MAX_SYNC_MESSAGE_SIZE)
            .await
            .map_err(|e| format!("read reverse update failed: {}", e))?;

        let received_bytes = remote_update.len();

        if !remote_update.is_empty() {
            local_doc
                .import(&remote_update)
                .map_err(|e| format!("loro import reverse failed: {}", e))?;
        }

        // 完成信号：import 落地后才 FIN stream2 发送侧，客户端读到 EOF
        // 才会返回（确保服务端先于客户端收尾）。FIN 本身经连接滞留窗口
        // 保障送达。
        send2
            .finish()
            .map_err(|e| format!("finish completion signal failed: {}", e))?;
        linger_conn(&conn);

        debug!(
            "accept_sync completed: sent={} bytes, received={} bytes",
            sent_bytes, received_bytes
        );

        Ok(SyncReport {
            sent_bytes,
            received_bytes,
            remote_peer: "incoming".to_string(),
            success: true,
            error: None,
        })
    }

    /// 关闭 iroh Endpoint，释放端口与资源。
    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

/// 后台同步循环：在独立 tokio 任务中持续接受入站同步。
///
/// 错误时记录日志并继续（V19 §14.2.1 离线队列保证最终一致性）。
pub async fn run_accept_loop(
    transport: Arc<IrohTransport>,
    doc: Arc<LoroDoc>,
    shutdown: tokio::sync::watch::Receiver<bool>,
) {
    info!("iroh accept loop started");

    loop {
        // 检查关闭信号
        if *shutdown.borrow() {
            info!("iroh accept loop shutting down");
            break;
        }

        match transport.accept_sync(&doc).await {
            Ok(report) => {
                debug!(
                    "accept loop: sync success sent={} recv={}",
                    report.sent_bytes, report.received_bytes
                );
            }
            Err(e) => {
                warn!("accept loop: sync error: {}", e);
                // 短暂退避后继续
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_frame_roundtrip() {
        let payload = b"hello aurora sync";
        let frame = encode_frame(payload);
        assert!(frame.len() > 4);

        let (decoded, remaining) = decode_frame(&frame).expect("decode");
        assert_eq!(decoded, payload);
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_decode_frame_partial_data_returns_none() {
        // 只有 2 字节，不足长度前缀
        assert!(decode_frame(&[0, 1]).is_none());

        // 长度声明 100 但无 payload
        let len = 100u32.to_be_bytes();
        assert!(decode_frame(&len).is_none());
    }

    #[test]
    fn test_decode_frame_multiple_frames() {
        let payload_a = b"first";
        let payload_b = b"second";

        let mut buf = encode_frame(payload_a);
        buf.extend_from_slice(&encode_frame(payload_b));

        // 解析第一帧
        let (decoded_a, rest) = decode_frame(&buf).expect("first frame");
        assert_eq!(decoded_a, payload_a);

        // 解析第二帧
        let (decoded_b, remaining) = decode_frame(rest).expect("second frame");
        assert_eq!(decoded_b, payload_b);
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_aurora_alpn_constant() {
        assert_eq!(AURORA_ALPN, b"aurora-note/1");
        assert_eq!(MAX_SYNC_MESSAGE_SIZE, 10 * 1024 * 1024);
    }

    // ===== DK-08 §7.4：多节点多 NAT 仿真测试 =====
    //
    // 仿真模型：iroh 官方内存测试网络 `TestNetwork`（in-memory 通道替代
    // 真实 socket）——每个 Endpoint 独立身份 = 各自独立 NAT 后的节点，
    // 节点间不经任何发现/中转服务，`EndpointAddr` 由测试代码手工交换
    // （对应生产中设备地址经二维码/账户服务带外交换的模型）。
    //
    // 选型说明：真实 UDP 网络在 CI 沙箱不可用（非 lo 接口被防火墙拦截，
    // 直连路径打开后被弃导致路径饿死）；本地 relay 会合可行但依赖路径
    // 状态机收敛。内存网络完全确定、无防火墙语义，是 §7.4「多节点拓扑
    // + CRDT 收敛」验收的合适载体。
    //
    // 验收核心：多轮两两同步后**全节点 CRDT 文本收敛一致**（多跳传播）。

    use std::time::Duration;
    use tokio::time::timeout;

    use iroh::test_utils::test_transport::TestNetwork;

    const SYNC_TIMEOUT_SECS: u64 = 15;

    /// 每测试一个内存网络（测试结束自动丢弃）。
    fn spawn_network() -> TestNetwork {
        TestNetwork::new()
    }

    /// 节点绑定：确定性密钥（按 tag 派生）+ 内存传输 + 网内地址查找。
    ///
    /// 直接构造 `IrohTransport`（同模块可访问私有字段），避免为测试改动
    /// 生产构造器。
    async fn spawn_node(network: &TestNetwork, tag: &str) -> Arc<IrohTransport> {
        // 确定性 32 字节密钥：同 tag 同身份（可复现）
        let mut key = [0u8; 32];
        for (i, b) in tag.bytes().enumerate() {
            key[i] = b;
        }
        key[31] = tag.len() as u8;
        let secret = iroh::SecretKey::from_bytes(&key);

        let transport = network
            .create_transport(secret.public())
            .expect("create test transport");
        // Minimal 补 crypto provider；TestTransport 预设挂内存传输 + 网内地址查找
        let endpoint = Endpoint::builder(Minimal)
            .secret_key(secret)
            .alpns(vec![AURORA_ALPN.to_vec()])
            .clear_ip_transports()
            .preset(transport)
            .bind()
            .await
            .unwrap_or_else(|e| panic!("node {tag}: endpoint bind failed: {e}"));
        Arc::new(IrohTransport {
            endpoint,
            peer_id: Mutex::new(PeerId::from_str(&format!("node-{tag}"))),
        })
    }

    /// 各带一处本地编辑的文档（tag 即编辑内容）。
    fn edited_doc(tag: &str) -> Arc<LoroDoc> {
        let doc = LoroDoc::new();
        doc.get_text("content")
            .insert(0, &format!("[{tag}]"))
            .expect("insert text");
        doc.commit();
        Arc::new(doc)
    }

    /// 派生 n 个 accept_sync 服务任务（每任务处理恰好一个入站连接）。
    fn spawn_accepts(
        t: &Arc<IrohTransport>,
        doc: &Arc<LoroDoc>,
        n: usize,
    ) -> Vec<tokio::task::JoinHandle<Result<SyncReport, String>>> {
        (0..n)
            .map(|_| {
                let t = t.clone();
                let d = doc.clone();
                tokio::spawn(async move { t.accept_sync(&d).await })
            })
            .collect()
    }

    /// 客户端角色同步（超时防挂死 CI）。
    async fn sync_pair(client: &IrohTransport, addr: &EndpointAddr, doc: &LoroDoc) -> SyncReport {
        timeout(
            Duration::from_secs(SYNC_TIMEOUT_SECS),
            client.sync_with_peer(addr.clone(), doc),
        )
        .await
        .expect("client sync timeout")
        .expect("client sync failed")
    }

    /// 回收服务端任务：必须全部成功完成（客户端未连上会在此超时暴露）。
    async fn join_accepts(handles: Vec<tokio::task::JoinHandle<Result<SyncReport, String>>>) {
        for h in handles {
            let report = timeout(Duration::from_secs(SYNC_TIMEOUT_SECS), h)
                .await
                .expect("accept timeout (client never connected?)")
                .expect("accept task panicked")
                .expect("accept failed");
            assert!(report.success, "accept error: {:?}", report.error);
        }
    }

    fn doc_text(doc: &LoroDoc) -> String {
        doc.get_text("content").to_string()
    }

    /// 断言全部节点文本一致（CRDT 收敛）且包含每节点编辑。
    fn assert_converged(docs: &[Arc<LoroDoc>]) {
        let expected = doc_text(&docs[0]);
        assert!(!expected.is_empty());
        for (i, d) in docs.iter().enumerate() {
            assert_eq!(
                doc_text(d),
                expected,
                "node {i} 未收敛: {:?} vs {:?}",
                doc_text(d),
                expected
            );
        }
    }

    /// 星型拓扑收敛（§7.4 验收主体）：辐条→中心（第 1 轮）+ 中心→辐条扇出（第 2 轮）。
    async fn star_convergence(network: &TestNetwork, spoke_count: usize) {
        let hub = spawn_node(network, "hub").await;
        let mut spokes = Vec::with_capacity(spoke_count);
        for i in 0..spoke_count {
            spokes.push(spawn_node(network, &format!("s{i}")).await);
        }
        let hub_doc = edited_doc("H");
        let mut docs = vec![hub_doc.clone()];
        for i in 0..spoke_count {
            docs.push(edited_doc(&format!("S{i}")));
        }
        let spoke_docs: Vec<_> = docs[1..].to_vec();

        // 第 1 轮：辐条 → 中心，中心文档拿全全部编辑
        let accepts = spawn_accepts(&hub, &hub_doc, spoke_count);
        for (i, (t, d)) in spokes.iter().zip(&spoke_docs).enumerate() {
            let r = sync_pair(t, &hub.addr(), d).await;
            assert!(r.success, "spoke {i} → hub failed: {:?}", r.error);
            assert!(
                r.sent_bytes > 0 && r.received_bytes > 0,
                "双向流必须都有数据"
            );
        }
        join_accepts(accepts).await;
        let hub_text = doc_text(&hub_doc);
        assert!(hub_text.contains("[H]"));
        for i in 0..spoke_count {
            assert!(
                hub_text.contains(&format!("[S{i}]")),
                "hub 缺少 spoke {i} 的编辑"
            );
        }

        // 第 2 轮：中心 → 辐条扇出，全节点收敛
        for (t, d) in spokes.iter().zip(&spoke_docs) {
            let accepts = spawn_accepts(t, d, 1);
            let r = sync_pair(&hub, &t.addr(), &hub_doc).await;
            assert!(r.success, "hub → spoke failed: {:?}", r.error);
            join_accepts(accepts).await;
        }
        assert_converged(&docs);
    }

    /// §7.4 验收 1：两节点双向同步收敛（基线）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn dk08_p2p_two_node_bidirectional_converge() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .try_init();
        let network = spawn_network();
        let a = spawn_node(&network, "a").await;
        let b = spawn_node(&network, "b").await;
        let doc_a = edited_doc("A");
        let doc_b = edited_doc("B");

        let accepts = spawn_accepts(&b, &doc_b, 1);
        let report = sync_pair(&a, &b.addr(), &doc_a).await;
        assert!(report.success, "{:?}", report.error);
        join_accepts(accepts).await;

        // 双向收敛：双方文本一致且互含对方编辑
        let ta = doc_text(&doc_a);
        let tb = doc_text(&doc_b);
        assert_eq!(ta, tb, "两节点文本必须收敛一致");
        assert!(ta.contains("[A]") && ta.contains("[B]"), "got: {ta:?}");
    }

    /// §7.4 验收 2：5 节点星型拓扑（下限验收场景）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn dk08_p2p_star_five_node_converge() {
        let network = spawn_network();
        star_convergence(&network, 4).await;
    }

    /// §7.4 验收 3：10 节点星型拓扑（上限验收场景）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn dk08_p2p_star_ten_node_converge() {
        let network = spawn_network();
        star_convergence(&network, 9).await;
    }

    /// §7.4 验收 4：4 节点环形拓扑两轮同步 —— 多跳传播（A 的编辑经
    /// 邻居逐跳到达对侧节点）后全收敛。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn dk08_p2p_ring_four_node_multihop_converge() {
        const N: usize = 4;
        let network = spawn_network();
        let mut nodes = Vec::with_capacity(N);
        let mut docs = Vec::with_capacity(N);
        for i in 0..N {
            nodes.push(spawn_node(&network, &format!("r{i}")).await);
            docs.push(edited_doc(&format!("R{i}")));
        }

        // 两轮环形两两同步：i ↔ (i+1) % N（环形直径 2 → 两轮必收敛）
        for _round in 0..2 {
            for i in 0..N {
                let j = (i + 1) % N;
                let accepts = spawn_accepts(&nodes[j], &docs[j], 1);
                let r = sync_pair(&nodes[i], &nodes[j].addr(), &docs[i]).await;
                assert!(r.success, "ring {i}→{j} failed: {:?}", r.error);
                join_accepts(accepts).await;
            }
        }
        assert_converged(&docs);
    }
}
