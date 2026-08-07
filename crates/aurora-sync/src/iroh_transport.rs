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
use iroh::endpoint::presets::Empty;
use iroh::{Endpoint, EndpointAddr};
use loro::LoroDoc;

use crate::p2p::PeerId;

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
pub struct IrohTransport {
    /// iroh Endpoint（QUIC 监听器 + NAT 穿透）。
    endpoint: Endpoint,
    /// 本节点的 PeerId（映射到 iroh NodeId）。
    peer_id: Mutex<PeerId>,
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
    /// 使用 `Endpoint::empty()` 预设，自动完成基本绑定。
    pub async fn new(peer_id: PeerId) -> Result<Self, String> {
        let endpoint = Endpoint::builder(Empty)
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

        // 6. 反向同步：新双向流，发送本地缺失更新
        let (mut send2, mut recv2) = conn
            .open_bi()
            .await
            .map_err(|e| format!("open_bi reverse failed: {}", e))?;

        // 接收远端版本向量
        let remote_vv_data = recv2
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
        send2
            .write_all(&local_update)
            .await
            .map_err(|e| format!("write reverse update failed: {}", e))?;
        send2
            .finish()
            .map_err(|e| format!("finish reverse send failed: {}", e))?;

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

        // 反向：接收远端缺失更新
        let (mut send2, mut recv2) = conn
            .accept_bi()
            .await
            .map_err(|e| format!("accept_bi reverse failed: {}", e))?;

        // 发送本地版本向量
        let local_vv = local_doc.oplog_vv();
        let vv_bytes = local_vv.encode();
        let frame = encode_frame(&vv_bytes);
        send2
            .write_all(&frame)
            .await
            .map_err(|e| format!("write vv reverse failed: {}", e))?;
        send2
            .finish()
            .map_err(|e| format!("finish vv reverse failed: {}", e))?;

        // 接收远端增量
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
}
