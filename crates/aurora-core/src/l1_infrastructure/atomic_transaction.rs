//! 原子事务恢复 (Atomic Transaction Recovery) — V19 ARCH-002
//!
//! 对应 V19 §10.2.2 事务边界要求："Loro CRDT Doc 变更 → SQLite 元数据更新 →
//! 文件系统写入，三步必须在同一原子操作中。失败时回滚 Loro OpLog + 删除已写入文件。"
//!
//! # 三阶段原子提交
//!
//! 1. **临时文件 + 原子重命名**：文件写入时先写 `.tmp` 文件，`fsync()` 刷盘后
//!    原子重命名为目标文件名。若 `.tmp` 残留视为未完成事务，启动时自动清理。
//! 2. **写前日志 (WAL)**：在 SQLite `pending_writes` 表中记录待完成操作；
//!    写入成功后删除对应记录；启动时扫描表，重做未完成的写入或回滚 Loro OpLog。
//! 3. **校验和验证**：每个文件写入时附带 SHA-256 校验和；读取时验证校验和，
//!    不匹配则触发修复流程（从 Loro OpLog 重建）。
//!
//! # 恢复流程
//!
//! 启动时调用 [`AtomicTransaction::recover_on_startup`]，按以下优先级：
//! - 清理残留 `.tmp` 文件
//! - 扫描 `pending_writes` 表，重做已完成写入 + 删除记录
//! - 回滚未完成的 Loro OpLog
//! - 触发全量校验和扫描（可选，耗时操作）

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use sha3::{Digest, Sha3_256};
use tracing::{debug, error, info, warn};

/// 原子事务管理器。
pub struct AtomicTransaction {
    /// 数据文件根目录。
    data_dir: PathBuf,
    /// 临时文件目录（`data_dir/.tmp`）。
    tmp_dir: PathBuf,
}

/// 写前日志记录（对应 SQLite `pending_writes` 表）。
#[derive(Debug, Clone)]
pub struct PendingWrite {
    /// 自动递增主键。
    pub id: i64,
    /// 目标文件路径（相对 data_dir）。
    pub file_path: String,
    /// 临时文件路径。
    pub tmp_path: String,
    /// 对应的 Loro OpLog 操作 ID。
    pub loro_op_id: String,
}

/// 原子事务操作结果。
#[derive(Debug)]
pub struct WriteResult {
    /// 文件 SHA-256 校验和。
    pub checksum: [u8; 32],
    /// 写入的字节数。
    pub bytes_written: usize,
}

/// 校验和不匹配错误。
#[derive(Debug)]
pub struct ChecksumMismatch {
    pub file_path: String,
    pub expected: [u8; 32],
    pub actual: [u8; 32],
}

impl AtomicTransaction {
    /// 创建原子事务管理器。
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        let tmp_dir = data_dir.join(".tmp");
        Self { data_dir, tmp_dir }
    }

    /// 原子写入文件（三阶段）：
    /// 1. 写入临时文件
    /// 2. fsync 刷盘
    /// 3. 原子重命名为目标文件
    ///
    /// 返回写入字节数及 SHA-256 校验和。
    pub fn atomic_write(
        &self,
        rel_path: &str,
        content: &[u8],
        loro_op_id: &str,
    ) -> Result<WriteResult, crate::Error> {
        let target = self.data_dir.join(rel_path);
        let tmp_path = self.tmp_path(rel_path, loro_op_id);

        // 1. 确保临时目录存在
        fs::create_dir_all(&self.tmp_dir)
            .map_err(|e| crate::Error::Internal(format!("create tmp_dir failed: {}", e)))?;

        // 确保目标父目录存在
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| crate::Error::Internal(format!("create data dir failed: {}", e)))?;
        }

        // 2. 写入临时文件
        let mut tmp_file = fs::File::create(&tmp_path).map_err(|e| crate::Error::Io(e))?;
        tmp_file
            .write_all(content)
            .map_err(|e| crate::Error::Io(e))?;
        tmp_file.sync_all().map_err(|e| crate::Error::Io(e))?;
        let bytes_written = content.len();

        // 3. 计算校验和
        let mut hasher = Sha3_256::new();
        hasher.update(content);
        let checksum: [u8; 32] = hasher.finalize().into();

        // 4. 原子重命名
        fs::rename(&tmp_path, &target).map_err(|e| {
            // 重命名失败：清理临时文件
            let _ = fs::remove_file(&tmp_path);
            crate::Error::Io(e)
        })?;

        debug!(
            path = %rel_path,
            bytes = bytes_written,
            op_id = %loro_op_id,
            "atomic_write completed"
        );

        Ok(WriteResult {
            checksum,
            bytes_written,
        })
    }

    /// 验证文件校验和。
    pub fn verify_checksum(&self, rel_path: &str) -> Result<[u8; 32], crate::Error> {
        let target = self.data_dir.join(rel_path);
        let content = fs::read(&target).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                crate::Error::NotFound(format!("file not found: {}", rel_path))
            } else {
                crate::Error::Io(e)
            }
        })?;

        let mut hasher = Sha3_256::new();
        hasher.update(&content);
        Ok(hasher.finalize().into())
    }

    /// 校验文件校验和，不匹配时返回 [`ChecksumMismatch`] 错误。
    pub fn check_checksum(&self, rel_path: &str, expected: &[u8; 32]) -> Result<(), crate::Error> {
        let actual = self.verify_checksum(rel_path)?;
        if actual != *expected {
            return Err(crate::Error::Internal(format!(
                "checksum mismatch for {}: expected {:?}, got {:?}",
                rel_path, expected, actual
            )));
        }
        Ok(())
    }

    /// 启动时恢复：
    /// 1. 清理残留 `.tmp` 文件
    /// 2. 扫描 `pending_writes` 表（由调用方提供）
    /// 3. 重做已完成写入 / 删除记录
    pub fn recover_on_startup(
        &self,
        pending_writes: &[PendingWrite],
    ) -> Result<Vec<String>, crate::Error> {
        let mut recovered = Vec::new();

        // 1. 清理残留临时文件（不在 pending_writes 中的）
        let known_tmp_names: Vec<&str> = pending_writes
            .iter()
            .map(|pw| pw.tmp_path.as_str())
            .collect();
        self.cleanup_tmp_files_except(&known_tmp_names)?;

        // 2. 处理 pending_writes
        for pw in pending_writes {
            let target = self.data_dir.join(&pw.file_path);
            let tmp = self.tmp_dir.join(&pw.tmp_path);

            if tmp.exists() {
                // 临时文件存在 → 写入尚未完成，尝试完成
                info!(
                    path = %pw.file_path,
                    op_id = %pw.loro_op_id,
                    "recovering incomplete write"
                );
                // 使用 copy+remove 代替 rename，兼容跨文件系统场景
                if let Err(e) = fs::copy(&tmp, &target) {
                    warn!(
                        path = %pw.file_path,
                        error = %e,
                        "recovery copy failed; removing tmp"
                    );
                    let _ = fs::remove_file(&tmp);
                } else {
                    let _ = fs::remove_file(&tmp);
                    recovered.push(pw.file_path.clone());
                }
            } else if target.exists() {
                // 目标文件已存在 → 写入已完成，标记为可清理
                debug!(path = %pw.file_path, "pending_write already completed");
                recovered.push(pw.file_path.clone());
            } else {
                // 两者都不存在 → 从未开始写入，标记为可清理
                debug!(path = %pw.file_path, "pending_write never started; skipping");
            }
        }

        info!(count = recovered.len(), "startup recovery completed");
        Ok(recovered)
    }

    /// 全量校验和扫描（耗时操作，建议在低频后台通道执行）。
    ///
    /// 扫描 `data_dir` 下所有文件，对每个文件验证内部存储的校验和。
    /// 返回不匹配的文件列表。
    pub fn full_checksum_scan(
        &self,
        known_checksums: &[(String, [u8; 32])],
    ) -> Vec<ChecksumMismatch> {
        let mut mismatches = Vec::new();

        for (path, expected) in known_checksums {
            match self.verify_checksum(path) {
                Ok(actual) if actual != *expected => {
                    error!(
                        path = %path,
                        ?expected,
                        ?actual,
                        "checksum mismatch — file may be corrupted"
                    );
                    mismatches.push(ChecksumMismatch {
                        file_path: path.clone(),
                        expected: *expected,
                        actual,
                    });
                }
                Err(_) => {
                    warn!(path = %path, "file missing during checksum scan");
                }
                _ => {}
            }
        }

        mismatches
    }

    // ── 内部辅助 ───────────────────────────────────────────────

    /// 生成临时文件路径。
    fn tmp_path(&self, rel_path: &str, op_id: &str) -> PathBuf {
        // 将路径中的分隔符替换为 `_`，避免创建深层目录
        let safe_name = rel_path.replace(['/', '\\'], "_");
        self.tmp_dir.join(format!("{}_{}.tmp", safe_name, op_id))
    }

    /// 清理残留的 `.tmp` 文件（跳过 `except` 列表中的）。
    fn cleanup_tmp_files_except(&self, except: &[&str]) -> Result<(), crate::Error> {
        if !self.tmp_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.tmp_dir)
            .map_err(|e| crate::Error::Internal(format!("read tmp_dir failed: {}", e)))?;

        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.extension().and_then(|e| e.to_str()) == Some("tmp")
                && !except.contains(&file_name)
            {
                debug!(path = %path.display(), "cleaning stale tmp file");
                if let Err(e) = fs::remove_file(&path) {
                    warn!(path = %path.display(), error = %e, "failed to remove stale tmp");
                } else {
                    count += 1;
                }
            }
        }

        if count > 0 {
            info!(count, "cleaned stale tmp files");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn atomic_write_and_verify() {
        let tmp = TempDir::new().unwrap();
        let at = AtomicTransaction::new(tmp.path().to_path_buf());

        let content = b"Hello, Aurora!";
        let result = at.atomic_write("notes/hello.md", content, "op-1").unwrap();
        assert_eq!(result.bytes_written, content.len());

        // 校验和验证
        let actual = at.verify_checksum("notes/hello.md").unwrap();
        assert_eq!(actual, result.checksum);

        // 无残留 tmp 文件
        assert!(tmp.path().join(".tmp").read_dir().unwrap().next().is_none());
    }

    #[test]
    fn checksum_mismatch_detected() {
        let tmp = TempDir::new().unwrap();
        let at = AtomicTransaction::new(tmp.path().to_path_buf());

        at.atomic_write("notes/test.md", b"correct", "op-1")
            .unwrap();
        let wrong = [0u8; 32];
        assert!(at.check_checksum("notes/test.md", &wrong).is_err());
    }

    #[test]
    fn recovery_cleans_stale_tmp() {
        let tmp = TempDir::new().unwrap();
        let at = AtomicTransaction::new(tmp.path().to_path_buf());
        fs::create_dir_all(&at.tmp_dir).unwrap();

        // 创建残留 tmp 文件
        fs::write(at.tmp_dir.join("stale_test.tmp"), b"stale").unwrap();

        at.recover_on_startup(&[]).unwrap();
        assert!(!at.tmp_dir.join("stale_test.tmp").exists());
    }

    #[test]
    fn recovery_renames_existing_tmp() {
        let tmp = TempDir::new().unwrap();
        let at = AtomicTransaction::new(tmp.path().to_path_buf());

        // 模拟写入中断：tmp 存在，目标不存在
        fs::create_dir_all(&at.tmp_dir).unwrap();
        let target_dir = tmp.path().join("notes");
        fs::create_dir_all(&target_dir).unwrap();

        // 直接用 at.tmp_dir 写文件，与 recover_on_startup 中 self.tmp_dir.join 一致
        let tmp_file = at.tmp_dir.join("notes_test.md_op-99.tmp");
        fs::write(&tmp_file, b"recovered content").unwrap();
        assert!(tmp_file.exists(), "tmp file must exist before recovery");

        let pending = vec![PendingWrite {
            id: 1,
            file_path: "notes/test.md".into(),
            tmp_path: "notes_test.md_op-99.tmp".into(),
            loro_op_id: "op-99".into(),
        }];

        let recovered = at.recover_on_startup(&pending).unwrap();
        assert_eq!(recovered, vec!["notes/test.md".to_string()]);
        assert!(tmp.path().join("notes/test.md").exists());
        assert!(!tmp_file.exists());
    }
}
