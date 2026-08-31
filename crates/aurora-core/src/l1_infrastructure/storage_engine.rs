//! 三步原子事务编排器 — V20 P0-4（GAP-04）/ V19 §10.2.2 / ARCH-002
//!
//! 将「WAL 记录 → 元数据提交 → 文件系统原子写 → WAL 清理」编排为
//! 可崩溃恢复的原子操作，配合作 [`AtomicTransaction`]（tmp+fsync+rename）。
//!
//! # 编排协议（V20 §4.4）
//!
//! ```text
//! begin_pending(op)           ← 步骤1: WAL 记录（KV: pending_writes:{op}）
//!   ├─ metadata_commit        ← 步骤2: 元数据负载（KV: meta:{op}）
//!   ├─ fs_atomic_write        ← 步骤3: tmp → fsync → rename（唯一可能中断步）
//! finish_pending(op)          ← 步骤4: 清 WAL
//! ```
//!
//! Loro commit（CRDT 写入）由调用方在 [`StorageEngine::commit_atomic`] 之前
//! 完成——CRDT OpLog 天然幂等可重放，恢复时内容侧由 OpLog 重建兜底。
//!
//! # 崩溃恢复（recover_on_boot）
//!
//! - WAL 无记录 → 事务已完整提交（或从未开始）
//! - WAL 存在 + tmp 残留 → 写入未完成 → 回滚（删 tmp + 清 WAL，内容走 OpLog 重放）
//! - WAL 存在 + 目标存在 + 校验和通过 → 步骤 3 完成但 WAL 未清 → 补 finish（幂等收敛）
//! - WAL 存在 + 校验和不匹配 → 上报 mismatch（保留 WAL，上层从 OpLog 重建）
//!
//! # 故障注入测试
//!
//! [`FaultPoint`] 在步骤间注入 `Err`，测试覆盖全部中断点，断言恢复后
//! 状态收敛（零残留 tmp / WAL 清空 / 数据一致）。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use tracing::{info, warn};

use crate::l1_infrastructure::atomic_transaction::AtomicTransaction;
use crate::traits::kv_store::KVStore;

/// WAL 键前缀（KV 命名空间）。
pub const PENDING_PREFIX: &str = "pending_writes:";
/// 元数据键前缀。
pub const META_PREFIX: &str = "meta:";

/// 待完成事务的 WAL 记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRecord {
    /// 操作 ID（唯一，映射 AtomicTransaction 的 loro_op_id）。
    pub op_id: String,
    /// 相对文件路径（data_dir 内）。
    pub rel_path: String,
    /// 内容 SHA3-256 校验和（hex，64 字符）。
    pub checksum_hex: String,
    /// 元数据负载（步骤 2 提交内容，恢复时核对）。
    pub metadata: Vec<u8>,
    /// 创建时间（RFC3339）。
    pub created_at: String,
}

/// 恢复报告。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecoverReport {
    /// 补 finish 收敛的事务（fs 已完成、WAL 未清）。
    pub completed: Vec<String>,
    /// 回滚的事务（tmp 残留或从未开始；内容由 Loro OpLog 重放恢复）。
    pub rolled_back: Vec<String>,
    /// 校验和不匹配、需上层从 OpLog 重建的文件（WAL 保留）。
    pub mismatched: Vec<String>,
}

/// 故障注入点（测试专用；生产 [`StorageEngine::commit_atomic`] 恒通过）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    /// WAL 写入后、元数据提交前中断。
    AfterBegin,
    /// 元数据提交后、文件写入前中断。
    AfterMetadata,
    /// 文件写入后、WAL 清理前中断。
    AfterFsWrite,
}

/// 三步原子事务编排器。
///
/// 泛型 `K: KVStore` 承载 WAL 与元数据（生产为 SQLite 适配器，测试为内存实现）；
/// 文件侧复用 [`AtomicTransaction`] 的 tmp+fsync+rename 原语。
pub struct StorageEngine<K: KVStore> {
    kv: K,
    fs: AtomicTransaction,
    data_dir: std::path::PathBuf,
    /// 故障注入钩子（`Option` 零成本；生产恒 None 不触发）。
    fault: std::sync::Mutex<Option<FaultPoint>>,
}

impl<K: KVStore> StorageEngine<K> {
    /// 创建编排器。`data_dir` 为文件写入根目录。
    pub fn new(kv: K, data_dir: impl Into<std::path::PathBuf>) -> Self {
        let data_dir = data_dir.into();
        let fs = AtomicTransaction::new(data_dir.clone());
        Self {
            kv,
            fs,
            data_dir,
            fault: std::sync::Mutex::new(None),
        }
    }

    /// 测试用：启用故障注入（生产代码不应调用）。
    pub fn inject_fault(&self, fp: FaultPoint) {
        *self.fault.lock().unwrap() = Some(fp);
    }

    /// 清除故障注入。
    pub fn clear_fault(&self) {
        *self.fault.lock().unwrap() = None;
    }

    /// 故障闸门：命中注入点返回 Err（模拟该步中断）。
    fn trip(&self, fp: FaultPoint) -> Result<(), crate::Error> {
        if *self.fault.lock().unwrap() == Some(fp) {
            return Err(crate::Error::Internal(format!(
                "injected fault at {:?}",
                fp
            )));
        }
        Ok(())
    }

    /// 完整三步原子提交（Loro commit 须由调用方在此前完成）。
    ///
    /// 步骤 1: WAL 记录（begin — 先记后做）
    /// 步骤 2: 元数据负载提交
    /// 步骤 3: 文件 tmp→fsync→rename
    /// 步骤 4: 清 WAL（finish）
    pub async fn commit_atomic(
        &self,
        op_id: &str,
        rel_path: &str,
        content: &[u8],
        metadata: &[u8],
    ) -> Result<(), crate::Error> {
        // 安全校验：拒绝路径穿越（与 AtomicTransaction 同规则）
        if rel_path.contains("..") {
            return Err(crate::Error::InvalidInput(format!(
                "rel_path contains '..' (path traversal blocked): {}",
                rel_path
            )));
        }

        // ── 步骤 1: begin — WAL 写入 ──
        let rec = PendingRecord {
            op_id: op_id.to_string(),
            rel_path: rel_path.to_string(),
            checksum_hex: hex_sha3(content),
            metadata: metadata.to_vec(),
            created_at: now_rfc3339(),
        };
        let wal_key = wal_key_of(op_id);
        self.kv
            .set(&wal_key, &serde_json::to_vec(&rec)?)
            .await?;

        self.trip(FaultPoint::AfterBegin)?;

        // ── 步骤 2: 元数据提交（WAL 先行保证崩溃可恢复） ──
        let meta_key = format!("{}{}", META_PREFIX, op_id);
        self.kv.set(&meta_key, &rec.metadata).await?;

        self.trip(FaultPoint::AfterMetadata)?;

        // ── 步骤 3: 文件系统原子写 ──
        self.fs.atomic_write(rel_path, content, op_id)?;

        self.trip(FaultPoint::AfterFsWrite)?;

        // ── 步骤 4: finish — 清 WAL ──
        self.kv.delete(&wal_key).await?;
        Ok(())
    }

    /// 启动恢复 — 重放/回滚未完成事务（V20 §4.4 recover_on_boot）。
    pub async fn recover_on_boot(&self) -> Result<RecoverReport, crate::Error> {
        let mut report = RecoverReport::default();
        let pendings = self.kv.scan_prefix(PENDING_PREFIX).await?;
        for (key, val) in pendings {
            let rec: PendingRecord = match serde_json::from_slice(&val) {
                Ok(r) => r,
                Err(e) => {
                    warn!(key = %key, error = %e, "undecodable pending record; purging");
                    self.kv.delete(&key).await?;
                    continue;
                }
            };
            let target = self.data_dir.join(&rec.rel_path);
            let tmp = self.tmp_path_of(&rec);

            if tmp.exists() {
                // 写入未完成 → 回滚（内容由 Loro OpLog 重放恢复）
                let _ = std::fs::remove_file(&tmp);
                self.kv.delete(&key).await?;
                report.rolled_back.push(rec.rel_path);
            } else if target.exists() {
                // 目标已存在 → 校验和复核后补 finish
                match self
                    .fs
                    .check_checksum(&rec.rel_path, &hex_to_32bytes(&rec.checksum_hex))
                {
                    Ok(()) => {
                        self.kv.delete(&key).await?;
                        report.completed.push(rec.rel_path);
                    }
                    Err(_) => {
                        // 校验失败 → 保留 WAL 待上层从 OpLog 重建
                        report.mismatched.push(rec.rel_path);
                    }
                }
            } else {
                // 从未开始写入 → 清 WAL（下次保存重写）
                self.kv.delete(&key).await?;
                report.rolled_back.push(rec.rel_path);
            }
        }
        info!(
            completed = report.completed.len(),
            rolled_back = report.rolled_back.len(),
            mismatched = report.mismatched.len(),
            "StorageEngine recovery finished"
        );
        Ok(report)
    }

    /// 与 `AtomicTransaction::tmp_path` 同构（其私有，此处对齐格式）:
    /// `{rel_path.replace('/','\\','_')}_{op_id}.tmp` under `.tmp/`。
    fn tmp_path_of(&self, rec: &PendingRecord) -> std::path::PathBuf {
        let safe_name = rec.rel_path.replace(['/', '\\'], "_");
        self.data_dir
            .join(".tmp")
            .join(format!("{}_{}.tmp", safe_name, rec.op_id))
    }
}

fn wal_key_of(op_id: &str) -> String {
    format!("{}{}", PENDING_PREFIX, op_id)
}

fn hex_sha3(data: &[u8]) -> String {
    let mut h = Sha3_256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_to_32bytes(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..32 {
        if let Ok(b) = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16) {
            out[i] = b;
        }
    }
    out
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// 内存 KVStore（开发/基准/测试用 — 非持久化）。
#[derive(Default)]
pub struct MemoryKVStore(std::sync::RwLock<std::collections::BTreeMap<String, Vec<u8>>>);

#[async_trait]
impl KVStore for MemoryKVStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, crate::Error> {
        Ok(self.0.read().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &[u8]) -> Result<(), crate::Error> {
        self.0.write().unwrap().insert(key.into(), value.to_vec());
        Ok(())
    }
    async fn delete(&self, key: &str) -> Result<(), crate::Error> {
        self.0.write().unwrap().remove(key);
        Ok(())
    }
    async fn exists(&self, key: &str) -> Result<bool, crate::Error> {
        Ok(self.0.read().unwrap().contains_key(key))
    }
    async fn batch_get(&self, keys: &[&str]) -> Result<Vec<Option<Vec<u8>>>, crate::Error> {
        let g = self.0.read().unwrap();
        Ok(keys.iter().map(|k| g.get(*k).cloned()).collect())
    }
    async fn batch_set(&self, items: &[(&str, &[u8])]) -> Result<(), crate::Error> {
        let mut g = self.0.write().unwrap();
        for (k, v) in items {
            g.insert((*k).into(), v.to_vec());
        }
        Ok(())
    }
    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, crate::Error> {
        let g = self.0.read().unwrap();
        Ok(g.iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine(dir: &tempfile::TempDir) -> StorageEngine<MemoryKVStore> {
        StorageEngine::new(MemoryKVStore::default(), dir.path())
    }

    // ── 正常路径 ──

    #[tokio::test]
    async fn happy_path_commits_and_clears_wal() {
        let dir = tempfile::tempdir().unwrap();
        let eng = make_engine(&dir);
        eng.commit_atomic("op1", "notes/a.md", b"hello", b"meta-1")
            .await
            .unwrap();
        // WAL 清空 + 文件落地 + 元数据在
        assert!(eng.kv.scan_prefix(PENDING_PREFIX).await.unwrap().is_empty());
        assert!(dir.path().join("notes/a.md").exists());
        assert_eq!(
            eng.kv.get("meta:op1").await.unwrap().as_deref(),
            Some(b"meta-1".as_slice())
        );
        // 再次恢复 → 无变化（幂等）
        let r = eng.recover_on_boot().await.unwrap();
        assert_eq!(r, RecoverReport::default());
    }

    // ── 故障注入: 步骤1后中断 ──

    #[tokio::test]
    async fn crash_after_begin_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let eng = make_engine(&dir);
        eng.inject_fault(FaultPoint::AfterBegin);
        assert!(
            eng.commit_atomic("op2", "notes/b.md", b"content", b"m")
                .await
                .is_err()
        );
        let report = eng.recover_on_boot().await.unwrap();
        assert_eq!(report.rolled_back, vec!["notes/b.md"]);
        assert!(eng.kv.scan_prefix(PENDING_PREFIX).await.unwrap().is_empty());
        assert!(!dir.path().join("notes/b.md").exists());
    }

    // ── 故障注入: 步骤2后中断 ──

    #[tokio::test]
    async fn crash_after_metadata_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let eng = make_engine(&dir);
        eng.inject_fault(FaultPoint::AfterMetadata);
        assert!(
            eng.commit_atomic("op3", "notes/c.md", b"x", b"m").await.is_err()
        );
        let report = eng.recover_on_boot().await.unwrap();
        assert_eq!(report.rolled_back.len(), 1);
        assert!(!dir.path().join("notes/c.md").exists());
    }

    // ── 故障注入: 步骤3后中断（文件已写、WAL 未清） ──

    #[tokio::test]
    async fn crash_after_fs_write_completes_on_boot() {
        let dir = tempfile::tempdir().unwrap();
        let eng = make_engine(&dir);
        eng.inject_fault(FaultPoint::AfterFsWrite);
        assert!(
            eng.commit_atomic("op4", "notes/d.md", b"final", b"m4")
                .await
                .is_err()
        );
        // 文件已落、WAL 残留
        assert!(dir.path().join("notes/d.md").exists());
        assert_eq!(eng.kv.scan_prefix(PENDING_PREFIX).await.unwrap().len(), 1);
        // 恢复 → 校验和通过 → 补 finish
        let report = eng.recover_on_boot().await.unwrap();
        assert_eq!(report.completed, vec!["notes/d.md"]);
        assert!(eng.kv.scan_prefix(PENDING_PREFIX).await.unwrap().is_empty());
        // 文件内容完好
        assert_eq!(std::fs::read(dir.path().join("notes/d.md")).unwrap(), b"final");
    }

    // ── 校验和不匹配 → 上报待重建 ──

    #[tokio::test]
    async fn mismatched_checksum_reported_for_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let eng = make_engine(&dir);
        eng.inject_fault(FaultPoint::AfterFsWrite);
        assert!(
            eng.commit_atomic("op5", "notes/e.md", b"v1", b"m").await.is_err()
        );
        // 模拟磁盘位翻转（外部篡改）
        std::fs::write(dir.path().join("notes/e.md"), b"corrupted").unwrap();
        let report = eng.recover_on_boot().await.unwrap();
        assert_eq!(report.mismatched, vec!["notes/e.md"]);
        // WAL 保留待上层从 OpLog 重建
        assert_eq!(eng.kv.scan_prefix(PENDING_PREFIX).await.unwrap().len(), 1);
    }

    // ── 路径穿越防护 ──

    #[tokio::test]
    async fn path_traversal_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let eng = make_engine(&dir);
        assert!(
            eng.commit_atomic("op6", "../evil.md", b"x", b"m")
                .await
                .is_err()
        );
        assert!(eng.kv.scan_prefix(PENDING_PREFIX).await.unwrap().is_empty());
    }

    // ── 多事务混合恢复 ──

    #[tokio::test]
    async fn mixed_transactions_recover_consistently() {
        let dir = tempfile::tempdir().unwrap();
        let eng = make_engine(&dir);

        // 事务A: 完整提交
        eng.commit_atomic("a", "notes/a.md", b"A", b"ma").await.unwrap();
        // 事务B: 中断在 fs 后（应补 finish）
        eng.inject_fault(FaultPoint::AfterFsWrite);
        let _ = eng.commit_atomic("b", "notes/b.md", b"B", b"mb").await;
        eng.clear_fault();
        // 事务C: 中断在 begin 后（应回滚）
        eng.inject_fault(FaultPoint::AfterBegin);
        let _ = eng.commit_atomic("c", "notes/c.md", b"C", b"mc").await;
        eng.clear_fault();

        let report = eng.recover_on_boot().await.unwrap();
        assert!(report.completed.contains(&"notes/b.md".to_string()));
        assert!(report.rolled_back.contains(&"notes/c.md".to_string()));
        assert_eq!(report.mismatched.len(), 0);
        // 最终: WAL 全清（mismatched 为空），A/B 内容完好
        assert!(eng.kv.scan_prefix(PENDING_PREFIX).await.unwrap().is_empty());
        assert_eq!(std::fs::read(dir.path().join("notes/a.md")).unwrap(), b"A");
        assert_eq!(std::fs::read(dir.path().join("notes/b.md")).unwrap(), b"B");
        assert!(!dir.path().join("notes/c.md").exists());
    }
}
