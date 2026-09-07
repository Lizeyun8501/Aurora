//! 审计防篡改哈希链 — V23-I0 / T12
//!
//! 「审计日志不可篡改」是物理上不成立的承诺（冰冷理性派 P1-2 的
//! 正确裁决）——本模块提供的是**可检测**的防篡改链路（tamper-evident）：
//! 每条记录的 `hash = SHA-256(prev_hash || actor || action ||
//! resource_type || resource_id || details || created_at)`，
//! `prev_hash` 指向前一条。任何中间记录被篡改（改字段/删行/插行），
//! [`AuditChain::verify`] 重算全链即定位首个断点。
//!
//! # 与 V22.1 口径对齐
//!
//! - 承诺从「不可篡改」降级为「篡改必可检测」——用户可见文案同步
//! - 历史数据（prev_hash/hash 为 NULL）视为 legacy 段：校验跳过，
//!   从第一条非 NULL 记录重新起链（`prev_hash = GENESIS`）
//! - 语义措辞修正（V23 §5.6）: 防篡改**链路**，非不可篡改

use rusqlite_chain::Connection;
use sha2::{Digest, Sha256};

/// 链首 prev_hash 约定值。
pub const GENESIS: &str = "GENESIS";

/// 校验结果：链完整 / 首个断点。
#[derive(Debug, Clone, PartialEq)]
pub enum ChainVerdict {
    /// 全链校验通过（含 legacy 段跳过）。
    Intact { verified: usize, legacy_skipped: usize },
    /// 第 `id` 条记录哈希不匹配（重算值 ≠ 存储值）。
    Broken { id: i64, expected: String, found: String },
    /// 链断裂：第 `id` 条的 prev_hash ≠ 前一条的 hash。
    Disconnected { id: i64 },
}

/// SQLite 审计哈希链写入器。
pub struct AuditChain;

impl AuditChain {
    /// 追加一条审计记录（自动接链）。
    ///
    /// `created_at` 由调用方传入（RFC3339）以保证可复现——哈希覆盖
    /// 时间的**文本表示**，同一记录在任何机器重算结果一致。
    pub fn append(
        conn: &Connection,
        actor: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        details_json: &str,
        created_at: &str,
    ) -> Result<(i64, String), crate::Error> {
        let prev_hash: Option<String> = conn
            .query_row(
                "SELECT hash FROM audit_log WHERE hash IS NOT NULL ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite_chain::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(|e| crate::Error::Internal(format!("audit chain read: {e}")))?;
        let prev = prev_hash.unwrap_or_else(|| GENESIS.to_string());
        let hash = Self::compute(
            &prev, actor, action, resource_type, resource_id, details_json, created_at,
        );
        conn.execute(
            "INSERT INTO audit_log (actor, action, resource_type, resource_id, details, created_at, prev_hash, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite_chain::params![
                actor,
                action,
                resource_type,
                resource_id,
                details_json,
                created_at,
                prev,
                hash
            ],
        )
        .map_err(|e| crate::Error::Internal(format!("audit insert: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok((id, hash))
    }

    /// 计算单条记录哈希（字段拼接顺序即链规范 — 变更即破坏兼容）。
    pub fn compute(
        prev_hash: &str,
        actor: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        details_json: &str,
        created_at: &str,
    ) -> String {
        let mut h = Sha256::new();
        h.update(prev_hash.as_bytes());
        h.update(b"\x1f");
        h.update(actor.as_bytes());
        h.update(b"\x1f");
        h.update(action.as_bytes());
        h.update(b"\x1f");
        h.update(resource_type.as_bytes());
        h.update(b"\x1f");
        h.update(resource_id.as_bytes());
        h.update(b"\x1f");
        h.update(details_json.as_bytes());
        h.update(b"\x1f");
        h.update(created_at.as_bytes());
        format!("{:x}", h.finalize())
    }

    /// 全链校验：重算每条哈希 + 检查 prev_hash 连接性。
    ///
    /// legacy 段（hash IS NULL）跳过并计数；首个非 NULL 记录视为
    /// 新链头（其 prev_hash 允许为任意值——legacy 尾或 GENESIS）。
    pub fn verify(conn: &Connection) -> Result<ChainVerdict, crate::Error> {
        let mut stmt = conn
            .prepare(
                "SELECT id, actor, action, resource_type, resource_id, details, created_at, prev_hash, hash
                 FROM audit_log ORDER BY id ASC",
            )
            .map_err(|e| crate::Error::Internal(format!("audit verify: {e}")))?;
        let mut rows = stmt
            .query([])
            .map_err(|e| crate::Error::Internal(format!("audit verify: {e}")))?;

        let mut verified = 0usize;
        let mut legacy = 0usize;
        let mut last_hash: Option<String> = None;
        while let Some(r) = rows
            .next()
            .map_err(|e| crate::Error::Internal(format!("audit verify: {e}")))?
        {
            let row_err = |e: rusqlite_chain::Error| {
                crate::Error::Internal(format!("audit verify row: {e}"))
            };
            let id: i64 = r.get(0).map_err(row_err)?;
            let actor: String = r.get(1).map_err(row_err)?;
            let action: String = r.get(2).map_err(row_err)?;
            let rt: String = r.get(3).map_err(row_err)?;
            let rid: String = r.get(4).map_err(row_err)?;
            let details: String = r.get(5).map_err(row_err)?;
            let created: String = r.get(6).map_err(row_err)?;
            let prev: Option<String> = r.get(7).map_err(row_err)?;
            let stored: Option<String> = r.get(8).map_err(row_err)?;

            let Some(stored_hash) = stored else {
                legacy += 1;
                continue; // legacy 段
            };

            // 连接性: prev_hash 必须接上前一条（或链头任意起）
            if let (Some(expect_prev), Some(actual_prev)) = (&last_hash, &prev) {
                if expect_prev != actual_prev {
                    return Ok(ChainVerdict::Disconnected { id });
                }
            }

            let recomputed =
                Self::compute(&prev.unwrap_or_else(|| GENESIS.into()), &actor, &action, &rt, &rid, &details, &created);
            if recomputed != stored_hash {
                return Ok(ChainVerdict::Broken {
                    id,
                    expected: recomputed,
                    found: stored_hash,
                });
            }
            verified += 1;
            last_hash = Some(stored_hash);
        }
        Ok(ChainVerdict::Intact { verified, legacy_skipped: legacy })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                actor TEXT NOT NULL, action TEXT NOT NULL,
                resource_type TEXT NOT NULL, resource_id TEXT NOT NULL,
                details TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
                prev_hash TEXT, hash TEXT)",
            [],
        )
        .unwrap();
        conn
    }

    /// 追加-校验闭环：N 条记录链完整。
    #[test]
    fn append_then_verify_intact() {
        let conn = setup();
        for i in 0..5 {
            AuditChain::append(
                &conn, "user", "note.update", "note", &format!("n{i}"), r#"{"k":1}"#, "2026-09-07T00:00:00Z",
            )
            .unwrap();
        }
        match AuditChain::verify(&conn).unwrap() {
            ChainVerdict::Intact { verified, legacy_skipped } => {
                assert_eq!(verified, 5);
                assert_eq!(legacy_skipped, 0);
            }
            other => panic!("expected intact, got {other:?}"),
        }
    }

    /// 链接性：每条 prev_hash = 前一条 hash；首条 = GENESIS。
    #[test]
    fn chain_links_prev_to_hash() {
        let conn = setup();
        let (_, h1) = AuditChain::append(&conn, "u", "a", "note", "n1", "{}", "T1").unwrap();
        let (id2, h2) = AuditChain::append(&conn, "u", "a", "note", "n2", "{}", "T2").unwrap();
        let prev2: String = conn
            .query_row("SELECT prev_hash FROM audit_log WHERE id = ?1", [id2], |r| r.get(0))
            .unwrap();
        assert_eq!(prev2, h1);
        assert_ne!(h1, h2);
        let prev1: String = conn
            .query_row("SELECT prev_hash FROM audit_log WHERE rowid = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(prev1, GENESIS);
    }

    /// 篡改检测：中间记录 details 被改 → Broken 定位到该条。
    #[test]
    fn tampering_detected() {
        let conn = setup();
        for i in 0..3 {
            AuditChain::append(&conn, "u", "a", "note", &format!("n{i}"), r#"{"v":1}"#, "T").unwrap();
        }
        conn.execute("UPDATE audit_log SET details = '{\"v\":999}' WHERE id = 2", [])
            .unwrap();
        match AuditChain::verify(&conn).unwrap() {
            ChainVerdict::Broken { id, .. } => assert_eq!(id, 2),
            other => panic!("expected broken, got {other:?}"),
        }
    }

    /// 删行检测：链连接性断裂 → Disconnected。
    #[test]
    fn deletion_detected() {
        let conn = setup();
        for i in 0..4 {
            AuditChain::append(&conn, "u", "a", "note", &format!("n{i}"), "{}", "T").unwrap();
        }
        conn.execute("DELETE FROM audit_log WHERE id = 2", []).unwrap();
        match AuditChain::verify(&conn).unwrap() {
            ChainVerdict::Disconnected { id } => assert_eq!(id, 3),
            other => panic!("expected disconnected, got {other:?}"),
        }
    }

    /// legacy 段跳过：NULL 哈希历史记录不影响新链校验。
    #[test]
    fn legacy_rows_skipped() {
        let conn = setup();
        conn.execute(
            "INSERT INTO audit_log (actor, action, resource_type, resource_id, details, created_at)
             VALUES ('old', 'a', 'note', 'n0', '{}', 'T0')",
            [],
        )
        .unwrap();
        AuditChain::append(&conn, "u", "a", "note", "n1", "{}", "T1").unwrap();
        match AuditChain::verify(&conn).unwrap() {
            ChainVerdict::Intact { verified, legacy_skipped } => {
                assert_eq!((verified, legacy_skipped), (1, 1));
            }
            other => panic!("expected intact, got {other:?}"),
        }
    }

    /// 同字段重算确定性：同输入必同哈希（跨机器可复现）。
    #[test]
    fn deterministic() {
        let a = AuditChain::compute("GENESIS", "u", "a", "note", "n1", "{}", "T");
        let b = AuditChain::compute("GENESIS", "u", "a", "note", "n1", "{}", "T");
        assert_eq!(a, b);
        // 任一字段变化 → 哈希变（含 0x1f 分隔防拼接歧义）
        let c = AuditChain::compute("GENESIS", "u", "a", "note", "n1", "{ }", "T");
        assert_ne!(a, c);
        // 分隔符歧义防护: ("ab","c") 与 ("a","bc") 必须不同
        let d1 = AuditChain::compute("G", "ab", "c", "x", "y", "{}", "T");
        let d2 = AuditChain::compute("G", "a", "bc", "x", "y", "{}", "T");
        assert_ne!(d1, d2);
    }
}
