//! 云端同步 (Cloud Sync)
//!
//! 提供 WebSocket 实时推送 + HTTPS 批量上传的混合云端同步。
//!
//! # 零知识架构
//! 服务器仅存储加密后的密文 blob (ciphertext relay)，无法解密内容。
//! 客户端在上传前使用 DEK (Data Encryption Key) 加密，
//! 下载后由本地解密。服务器仅做版本号与元数据管理。
//!
//! # 实现说明
//! 本模块以内存哈希表模拟服务端密文中转存储。真实实现将
//! `relay` 内部操作替换为 HTTPS/WS 客户端调用即可，公开 API 保持不变。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// 云端同步配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    /// WebSocket 实时推送地址 (wss://)。
    pub ws_url: String,
    /// HTTPS 批量上传/下载地址。
    pub https_url: String,
    /// 服务端中转 blob 的最大批次大小 (字节)。
    pub max_batch_bytes: usize,
    /// 是否启用实时推送。
    pub realtime_push: bool,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            ws_url: "wss://relay.aurora.example/v1/ws".to_string(),
            https_url: "https://relay.aurora.example/v1/batch".to_string(),
            max_batch_bytes: 4 * 1024 * 1024,
            realtime_push: true,
        }
    }
}

/// 单个同步批次的元数据与密文载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBatch {
    /// 批次唯一 ID (幂等键)。
    pub batch_id: String,
    /// 文档 ID。
    pub doc_id: String,
    /// 加密后的 op 字节流 (服务器无法解密)。
    pub ciphertext: Vec<u8>,
    /// 客户端版本向量快照 (明文元数据，用于增量判断)。
    pub vv: HashMap<String, u64>,
    /// 时间戳。
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl SyncBatch {
    /// 创建新批次。
    pub fn new(doc_id: impl Into<String>, ciphertext: Vec<u8>) -> Self {
        Self {
            batch_id: uuid::Uuid::new_v4().to_string(),
            doc_id: doc_id.into(),
            ciphertext,
            vv: HashMap::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    /// 附带版本向量。
    pub fn with_vv(mut self, vv: HashMap<String, u64>) -> Self {
        self.vv = vv;
        self
    }

    /// 序列化为字节 (用于 HTTPS 上传)。
    pub fn encode(&self) -> crate::Result<Vec<u8>> {
        bincode::serialize(self).map_err(crate::Error::from)
    }

    /// 从字节反序列化。
    pub fn decode(bytes: &[u8]) -> crate::Result<Self> {
        bincode::deserialize(bytes).map_err(crate::Error::from)
    }

    /// 密文字节数。
    pub fn size_bytes(&self) -> usize {
        self.ciphertext.len()
    }
}

/// 云端同步引擎。
///
/// 模拟 WebSocket + HTTPS 批量通道与零知识密文中转。
/// 真实实现替换 `relay` 内部存储为 HTTP/WS 客户端调用。
pub struct CloudSyncEngine {
    config: CloudConfig,
    /// 模拟服务端密文存储：doc_id -> Vec<SyncBatch> (按时间顺序)。
    relay: Arc<RwLock<HashMap<String, Vec<SyncBatch>>>>,
    /// 本地已上传的最新版本号快照。
    local_vv: Arc<RwLock<HashMap<String, u64>>>,
}

impl CloudSyncEngine {
    pub fn new(config: CloudConfig) -> Self {
        Self {
            config,
            relay: Arc::new(RwLock::new(HashMap::new())),
            local_vv: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn config(&self) -> &CloudConfig {
        &self.config
    }

    /// 上传一个加密批次到云端中转 (HTTPS 批量)。
    pub fn upload_batch(&self, mut batch: SyncBatch) -> crate::Result<String> {
        if batch.size_bytes() > self.config.max_batch_bytes {
            return Err(crate::Error::InvalidInput(format!(
                "batch too large: {} > {}",
                batch.size_bytes(),
                self.config.max_batch_bytes
            )));
        }
        let batch_id = batch.batch_id.clone();
        debug!(
            "cloud upload: doc={} batch={} bytes={}",
            batch.doc_id,
            batch_id,
            batch.size_bytes()
        );
        // 同步本地 VV (取最大值)
        {
            let mut vv = self.local_vv.write();
            for (k, v) in &batch.vv {
                let entry = vv.entry(k.clone()).or_insert(0);
                if *v > *entry {
                    *entry = *v;
                }
            }
        }
        batch.timestamp = chrono::Utc::now();
        self.relay
            .write()
            .entry(batch.doc_id.clone())
            .or_default()
            .push(batch);
        info!("cloud upload ok: batch={}", batch_id);
        Ok(batch_id)
    }

    /// 下载某文档自指定版本之后的所有批次 (HTTPS 批量)。
    ///
    /// `since_vv` 表示调用方已拥有的版本；仅返回包含新版本的批次。
    pub fn download_batches(
        &self,
        doc_id: &str,
        since_vv: &HashMap<String, u64>,
    ) -> crate::Result<Vec<SyncBatch>> {
        let relay = self.relay.read();
        let batches = relay.get(doc_id).cloned().unwrap_or_default();
        let filtered: Vec<_> = batches
            .into_iter()
            .filter(|b| {
                // 批次中存在 since_vv 未覆盖的节点版本 => 需要下载
                !b.vv
                    .iter()
                    .all(|(k, v)| since_vv.get(k).copied().unwrap_or(0) >= *v)
            })
            .collect();
        debug!("cloud download: doc={} returned={}", doc_id, filtered.len());
        Ok(filtered)
    }

    /// WebSocket 实时推送：模拟向订阅者广播新批次通知。
    pub fn push_notification(&self, doc_id: &str, batch_id: &str) -> crate::Result<()> {
        if !self.config.realtime_push {
            return Ok(());
        }
        info!("cloud ws push: doc={} batch={}", doc_id, batch_id);
        Ok(())
    }

    /// 删除某文档的全部中转密文 (用于 GDPR 删除)。
    pub fn purge_document(&self, doc_id: &str) -> crate::Result<usize> {
        let mut relay = self.relay.write();
        let removed = relay.remove(doc_id).map(|v| v.len()).unwrap_or(0);
        Ok(removed)
    }

    /// 返回某文档当前云端最大版本号快照。
    pub fn remote_vv(&self, doc_id: &str) -> HashMap<String, u64> {
        let relay = self.relay.read();
        relay
            .get(doc_id)
            .and_then(|batches| batches.last())
            .map(|b| b.vv.clone())
            .unwrap_or_default()
    }

    /// 返回某文档云端存储的批次数。
    pub fn batch_count(&self, doc_id: &str) -> usize {
        self.relay.read().get(doc_id).map(|v| v.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_config_default() {
        let cfg = CloudConfig::default();
        assert!(cfg.ws_url.starts_with("wss://"));
        assert!(cfg.https_url.starts_with("https://"));
        assert!(cfg.realtime_push);
        assert!(cfg.max_batch_bytes > 0);
    }

    #[test]
    fn test_sync_batch_encode_decode_roundtrip() {
        let mut vv = HashMap::new();
        vv.insert("p1".to_string(), 5u64);
        let batch = SyncBatch::new("doc1", vec![1, 2, 3, 4]).with_vv(vv);
        let bytes = batch.encode().expect("encode");
        let decoded = SyncBatch::decode(&bytes).expect("decode");
        assert_eq!(decoded.doc_id, "doc1");
        assert_eq!(decoded.ciphertext, vec![1, 2, 3, 4]);
        assert_eq!(decoded.vv.get("p1").copied(), Some(5));
    }

    #[test]
    fn test_cloud_upload_and_download() {
        let engine = CloudSyncEngine::new(CloudConfig::default());
        let mut vv = HashMap::new();
        vv.insert("p1".to_string(), 1u64);
        let batch = SyncBatch::new("doc1", vec![9, 9, 9]).with_vv(vv);
        let id = engine.upload_batch(batch).expect("upload");
        assert!(!id.is_empty());
        assert_eq!(engine.batch_count("doc1"), 1);

        // 全新客户端 (空 vv) 应下载到该批次
        let downloaded = engine
            .download_batches("doc1", &HashMap::new())
            .expect("download");
        assert_eq!(downloaded.len(), 1);
        assert_eq!(downloaded[0].ciphertext, vec![9, 9, 9]);
    }

    #[test]
    fn test_cloud_batch_too_large_rejected() {
        let mut cfg = CloudConfig::default();
        cfg.max_batch_bytes = 8;
        let engine = CloudSyncEngine::new(cfg);
        let batch = SyncBatch::new("doc1", vec![0u8; 16]);
        let result = engine.upload_batch(batch);
        assert!(result.is_err());
    }

    #[test]
    fn test_cloud_download_since_vv_filter() {
        let engine = CloudSyncEngine::new(CloudConfig::default());
        let mut vv1 = HashMap::new();
        vv1.insert("p1".to_string(), 1u64);
        engine
            .upload_batch(SyncBatch::new("doc1", vec![1]).with_vv(vv1))
            .unwrap();
        let mut vv2 = HashMap::new();
        vv2.insert("p1".to_string(), 2u64);
        engine
            .upload_batch(SyncBatch::new("doc1", vec![2]).with_vv(vv2))
            .unwrap();

        // 调用方已拥有 p1=2，应下载到 0 个新批次
        let mut have = HashMap::new();
        have.insert("p1".to_string(), 2u64);
        let downloaded = engine.download_batches("doc1", &have).expect("download");
        assert_eq!(downloaded.len(), 0);

        // 调用方仅拥有 p1=1，应下载到 1 个新批次 (vv2)
        let mut have2 = HashMap::new();
        have2.insert("p1".to_string(), 1u64);
        let downloaded2 = engine.download_batches("doc1", &have2).expect("download");
        assert_eq!(downloaded2.len(), 1);
        assert_eq!(downloaded2[0].ciphertext, vec![2]);
    }

    #[test]
    fn test_cloud_purge_document() {
        let engine = CloudSyncEngine::new(CloudConfig::default());
        engine
            .upload_batch(SyncBatch::new("doc1", vec![1, 2]))
            .unwrap();
        engine
            .upload_batch(SyncBatch::new("doc1", vec![3, 4]))
            .unwrap();
        assert_eq!(engine.batch_count("doc1"), 2);
        let removed = engine.purge_document("doc1").expect("purge");
        assert_eq!(removed, 2);
        assert_eq!(engine.batch_count("doc1"), 0);
    }

    #[test]
    fn test_cloud_push_notification_disabled() {
        let mut cfg = CloudConfig::default();
        cfg.realtime_push = false;
        let engine = CloudSyncEngine::new(cfg);
        // realtime_push 关闭时应快速返回 Ok
        engine.push_notification("doc1", "b1").expect("no push");
    }

    #[test]
    fn test_cloud_remote_vv() {
        let engine = CloudSyncEngine::new(CloudConfig::default());
        let mut vv = HashMap::new();
        vv.insert("p1".to_string(), 7u64);
        engine
            .upload_batch(SyncBatch::new("doc1", vec![1]).with_vv(vv))
            .unwrap();
        let remote = engine.remote_vv("doc1");
        assert_eq!(remote.get("p1").copied(), Some(7));
        // 不存在的文档返回空
        assert!(engine.remote_vv("missing").is_empty());
    }
}
