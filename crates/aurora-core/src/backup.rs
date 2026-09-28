//! DK-17 S1：备份与灾难恢复核心 — SQLite 快照 / 完整性校验 / 恢复演练。
//!
//! - **附件零额外工作**：blob 在 KV `attachblob:{sha256}`（DK-09 内容寻址），
//!   随主库快照自动覆盖；
//! - **tantivy 索引不入备份**：可从事件流重建（projection 既有语义）；
//! - **vault 密钥不入备份**（S2 口令包裹裁决挂起）——恢复演练仅验证结构完整性；
//! - 调度：boot 水位触发（BootedApp 层），运行时定时器 S2。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use rusqlite::backup::Backup;
use rusqlite::OpenFlags;
use sha2::{Digest, Sha256};

use crate::Error;

/// 备份产物报告。
#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    pub path: PathBuf,
    pub sha256_hex: String,
    pub size_bytes: u64,
    pub duration_ms: u128,
}

/// 校验报告。
#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub integrity_ok: bool,
    pub sha256_hex: String,
    /// 传入期望值时给出对账结果；None = 未对账。
    pub sha_matched: Option<bool>,
}

fn sha256_file(p: &Path) -> Result<(String, u64), Error> {
    use std::io::Read;
    let mut f = std::fs::File::open(p).map_err(|e| Error::Database(format!("backup open: {e}")))?;
    let mut h = Sha256::new();
    let mut size = 0u64;
    let mut buf = [0u8; 65536];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| Error::Database(format!("backup read: {e}")))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        size += n as u64;
    }
    Ok((format!("{:x}", h.finalize()), size))
}

fn integrity_check(p: &Path) -> Result<bool, Error> {
    let conn = rusqlite::Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| Error::Database(format!("backup integrity open: {e}")))?;
    let r: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| Error::Database(format!("integrity_check: {e}")))?;
    Ok(r == "ok")
}

/// 快照备份：online backup → `.tmp` 原子 rename → 完整性内检 → SHA-256 报告。
pub fn snapshot_backup(src: &rusqlite::Connection, out_dir: &Path) -> Result<BackupReport, Error> {
    std::fs::create_dir_all(out_dir).map_err(|e| Error::Database(format!("backup mkdir: {e}")))?;
    // DK-17 S2：毫秒精度——同秒连续备份不再同名覆盖（秒级会丢快照）。
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| Error::Database(format!("clock: {e}")))?
        .as_millis();
    let final_path = out_dir.join(format!("aurora-backup-{ts}.db"));
    let tmp_path = out_dir.join(format!("aurora-backup-{ts}.db.tmp"));
    let t0 = Instant::now();

    let mut dst = rusqlite::Connection::open(&tmp_path)
        .map_err(|e| Error::Database(format!("backup dst open: {e}")))?;
    {
        let b =
            Backup::new(src, &mut dst).map_err(|e| Error::Database(format!("backup init: {e}")))?;
        b.run_to_completion(64, Duration::from_millis(5), None)
            .map_err(|e| Error::Database(format!("backup run: {e}")))?;
    }
    dst.close()
        .map_err(|(_, e)| Error::Database(format!("backup dst close: {e}")))?;

    if !integrity_check(&tmp_path)? {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(Error::Database(
            "backup integrity_check failed before rename".into(),
        ));
    }
    std::fs::rename(&tmp_path, &final_path)
        .map_err(|e| Error::Database(format!("backup rename: {e}")))?;

    let (sha256_hex, size_bytes) = sha256_file(&final_path)?;
    Ok(BackupReport {
        path: final_path,
        sha256_hex,
        size_bytes,
        duration_ms: t0.elapsed().as_millis(),
    })
}

/// 校验备份：integrity_check + SHA-256（可对账期望值）。
pub fn verify_backup(p: &Path, expected_sha: Option<&str>) -> Result<VerifyReport, Error> {
    let integrity_ok = integrity_check(p)?;
    let (sha256_hex, _) = sha256_file(p)?;
    let sha_matched = expected_sha.map(|e| e.eq_ignore_ascii_case(&sha256_hex));
    Ok(VerifyReport {
        integrity_ok,
        sha256_hex,
        sha_matched,
    })
}

/// DK-17 S2：备份轮转——保留最近 `keep` 份（按文件名排序，aurora-backup-<ts>.db
/// 字典序即时间序），超额最旧先删。返回删除的文件名列表。
///
/// # Errors
/// 目录读失败 / 单文件删除失败（汇总为 Database 错误——轮转失败不阻塞备份主流程，
/// 调用方 warn 留痕即可）。
pub fn rotate_backups(out_dir: &Path, keep: usize) -> Result<Vec<String>, Error> {
    let mut names: Vec<String> = std::fs::read_dir(out_dir)
        .map_err(|e| Error::Database(format!("rotate read_dir: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("aurora-backup-") && n.ends_with(".db"))
        .collect();
    names.sort(); // 字典序 = 时间序（同前缀同格式）
    if names.len() <= keep {
        return Ok(vec![]);
    }
    let mut removed = Vec::new();
    let excess = names.len().saturating_sub(keep);
    for name in names.into_iter().take(excess) {
        std::fs::remove_file(out_dir.join(&name))
            .map_err(|e| Error::Database(format!("rotate remove {name}: {e}")))?;
        removed.push(name);
    }
    Ok(removed)
}

/// 恢复：备份文件 → 目标连接反向灌入（演练/灾难恢复路径）。
pub fn restore_backup(backup_path: &Path, target: &mut rusqlite::Connection) -> Result<(), Error> {
    let src = rusqlite::Connection::open_with_flags(backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| Error::Database(format!("restore src open: {e}")))?;
    let b = Backup::new(&src, target).map_err(|e| Error::Database(format!("restore init: {e}")))?;
    b.run_to_completion(64, Duration::from_millis(5), None)
        .map_err(|e| Error::Database(format!("restore run: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l1_infrastructure::storage::SqliteStorage;
    use crate::traits::kv_store::KVStore;

    fn temp_db_dir(tag: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(format!("{tag}.db"));
        (dir, db)
    }

    /// 演练主链：写数据 → 快照 → 全新库恢复 → KV 一致（DoD「恢复全流程跑通」）。
    #[tokio::test]
    async fn backup_round_trip_restore() {
        let (_dir, db) = temp_db_dir("dk17-src");
        let src = SqliteStorage::new(&db).unwrap();
        src.set("k1", b"v1").await.unwrap();
        src.set("attachblob:abc", b"blob-bytes").await.unwrap();

        let out = _dir.path().join("backups");
        let report = src.with_conn(|c| snapshot_backup(c, &out)).unwrap();
        assert!(report.size_bytes > 0);
        assert!(report.path.extension().and_then(|e| e.to_str()) == Some("db"));

        // 全新库恢复 → KV 一致
        let dst = SqliteStorage::new_in_memory().unwrap();
        dst.with_conn_mut(|g| restore_backup(&report.path, g))
            .unwrap();
        assert_eq!(
            dst.get("k1").await.unwrap().as_deref(),
            Some(b"v1".as_ref())
        );
        assert_eq!(
            dst.get("attachblob:abc").await.unwrap().as_deref(),
            Some(b"blob-bytes".as_ref())
        );
    }

    /// 篡改检测：字节翻转 → SHA 对账失败（完整性告警语义）。
    #[tokio::test]
    async fn backup_tamper_detection() {
        let (_dir, db) = temp_db_dir("dk17-tamper");
        let src = SqliteStorage::new(&db).unwrap();
        src.set("k", b"v").await.unwrap();

        let out = _dir.path().join("backups");
        let report = src.with_conn(|c| snapshot_backup(c, &out)).unwrap();
        // 翻转文件中部 1 字节
        let mut bytes = std::fs::read(&report.path).unwrap();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        std::fs::write(&report.path, &bytes).unwrap();

        let vr = verify_backup(&report.path, Some(&report.sha256_hex)).unwrap();
        assert_eq!(vr.sha_matched, Some(false), "篡改后 SHA 必须对账失败");
    }

    /// 轮转：10 份 → keep 3 → 剩 3 且最新保留。
    #[tokio::test]
    async fn rotate_keeps_latest() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("backups");
        std::fs::create_dir_all(&out).unwrap();
        for ts in [
            1700000001u64,
            1700000002,
            1700000003,
            1700000004,
            1700000005,
        ] {
            let name = format!("aurora-backup-{ts}.db");
            std::fs::write(out.join(&name), b"x").unwrap();
        }
        let removed = rotate_backups(&out, 3).unwrap();
        assert_eq!(removed.len(), 2);
        let mut left: Vec<String> = std::fs::read_dir(&out)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "aurora-backup-1700000003.db",
                "aurora-backup-1700000004.db",
                "aurora-backup-1700000005.db"
            ],
            "最旧两份被轮转，最新三份保留"
        );
    }

    /// 正常产物：integrity ok + SHA 对账成功。
    #[tokio::test]
    async fn backup_verify_ok() {
        let (_dir, db) = temp_db_dir("dk17-ok");
        let src = SqliteStorage::new(&db).unwrap();
        src.set("a", b"b").await.unwrap();

        let out = _dir.path().join("backups");
        let report = src.with_conn(|c| snapshot_backup(c, &out)).unwrap();
        let vr = verify_backup(&report.path, Some(&report.sha256_hex)).unwrap();
        assert!(vr.integrity_ok);
        assert_eq!(vr.sha_matched, Some(true));
    }
}
