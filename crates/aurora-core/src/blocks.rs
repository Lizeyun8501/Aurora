//! 块级存储（V23-I2 / T14 — 双轨地基）
//!
//! 与 notes 并列而非替换：notes 仍是聚合视图与事实源入口，
//! blocks 为块级索引/编辑/未来块级 CRDT 的地基（规格包 M3-1）。
//!
//! 切块规则（MVP，确定性）：
//! - `#`~`######` 开头 → heading
//! - `- [ ]` / `- [x]` 开头 → task
//! - ``` 围栏 → code（开栏到闭栏整体一块）
//! - `>` 开头 → quote
//! - 其余非空行 → text（连续行合并为一段）
//!
//! 五步法承诺：本模块只做「新增 + 双写」，不改 notes 读路径。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// 切块产出的原始块（写入前的中间形态）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawBlock {
    pub block_type: &'static str,
    pub content: String,
}

/// Markdown → 块序列（确定性；两端同输入必同输出）。
pub fn split_markdown_blocks(content: &str) -> Vec<RawBlock> {
    let mut out: Vec<RawBlock> = Vec::new();
    let mut in_code = false;
    let mut para = String::new();

    let flush_para = |para: &mut String, out: &mut Vec<RawBlock>| {
        if !para.trim().is_empty() {
            out.push(RawBlock { block_type: "text", content: std::mem::take(para) });
        }
    };

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            flush_para(&mut para, &mut out);
            if in_code {
                // 闭栏行并入当前 code 块
                if let Some(last) = out.last_mut() {
                    if last.block_type == "code" {
                        last.content.push('\n');
                        last.content.push_str(line);
                    }
                }
                in_code = false;
            } else {
                out.push(RawBlock { block_type: "code", content: line.to_string() });
                in_code = true;
            }
            continue;
        }
        if in_code {
            if let Some(last) = out.last_mut() {
                if last.block_type == "code" {
                    last.content.push('\n');
                    last.content.push_str(line);
                }
            }
            continue;
        }
        if is_heading(trimmed) {
            flush_para(&mut para, &mut out);
            out.push(RawBlock { block_type: "heading", content: line.to_string() });
        } else if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            flush_para(&mut para, &mut out);
            out.push(RawBlock { block_type: "task", content: line.to_string() });
        } else if trimmed.starts_with('>') {
            flush_para(&mut para, &mut out);
            out.push(RawBlock { block_type: "quote", content: line.to_string() });
        } else if trimmed.is_empty() {
            flush_para(&mut para, &mut out);
        } else {
            if !para.is_empty() {
                para.push('\n');
            }
            para.push_str(line);
        }
    }
    if in_code {
        // 未闭合 code — 保持为一整块（容忍正在输入的编辑态）
    }
    flush_para(&mut para, &mut out);
    out
}

fn is_heading(line: &str) -> bool {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    hashes >= 1 && hashes <= 6 && line[hashes..].starts_with(' ')
}

/// 块记录（对应 blocks 表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRecord {
    pub id: String,
    pub note_id: String,
    pub block_type: String,
    pub content: String,
    pub position: f64,
    pub is_deleted: bool,
}

/// SQLite blocks 表存取（双轨写路径）。
pub struct BlockStore {
    conn: std::sync::Mutex<Connection>,
}

impl BlockStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn: std::sync::Mutex::new(conn) }
    }

    /// 同步一篇笔记的块集（幂等：软删旧块 + 插入新块集）。
    ///
    /// position = 行序 × 100.0（留插入空间 — REAL 排序契约）。
    /// 返回本次写入块数。
    pub fn sync_note_blocks(
        &self,
        note_id: &str,
        workspace_id: Option<&str>,
        content: &str,
    ) -> Result<usize, crate::Error> {
        let raw = split_markdown_blocks(content);
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(format!("blocks mutex: {e}")))?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE blocks SET is_deleted = 1, updated_at = ?2 WHERE note_id = ?1 AND is_deleted = 0",
            rusqlite::params![note_id, now],
        )
        .map_err(|e| crate::Error::Database(e.to_string()))?;
        for (idx, b) in raw.iter().enumerate() {
            conn.execute(
                "INSERT INTO blocks (id, note_id, workspace_id, block_type, content_json, position, created_at, updated_at, is_deleted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 0)",
                rusqlite::params![
                    format!("blk_{}_{}", short_hash(note_id, content), idx),
                    note_id,
                    workspace_id,
                    b.block_type,
                    serde_json::to_string(&b.content)
                        .map_err(|e| crate::Error::Serialization(e))?,
                    (idx as f64) * 100.0,
                    now,
                ],
            )
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        }
        Ok(raw.len())
    }

    /// 读取一篇笔记的活跃块（position 升序）。
    pub fn list_note_blocks(&self, note_id: &str) -> Result<Vec<BlockRecord>, crate::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(format!("blocks mutex: {e}")))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, note_id, block_type, content_json, position, is_deleted
                 FROM blocks WHERE note_id = ?1 AND is_deleted = 0 ORDER BY position ASC",
            )
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        let rows = stmt
            .query_map([note_id], |r| {
                let content_json: String = r.get(3)?;
                Ok(BlockRecord {
                    id: r.get(0)?,
                    note_id: r.get(1)?,
                    block_type: r.get(2)?,
                    content: serde_json::from_str(&content_json).unwrap_or_default(),
                    position: r.get(4)?,
                    is_deleted: r.get::<_, i64>(5)? != 0,
                })
            })
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| crate::Error::Database(e.to_string()))
    }
}

/// 块 ID 稳定短哈希（同 note+content → 同 id；重写内容变化则自然换 id）。
fn short_hash(note_id: &str, content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    note_id.hash(&mut h);
    content.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_rules_deterministic() {
        let md = "# 标题\n\n正文第一段\n续行\n- [ ] 待办 A\n- [x] 已办 B\n> 引用\n```rust\nfn main() {}\n```\n尾段";
        let a = split_markdown_blocks(md);
        let b = split_markdown_blocks(md);
        assert_eq!(a, b, "同输入必同输出");
        let kinds: Vec<_> = a.iter().map(|b| b.block_type).collect();
        assert_eq!(
            kinds,
            vec!["heading", "text", "task", "task", "quote", "code", "text"]
        );
        // code 块含闭栏
        assert!(a[5].content.contains("fn main") && a[5].content.contains("```"));
        // text 段合并续行
        assert_eq!(a[1].content, "正文第一段\n续行");
    }

    #[test]
    fn sync_then_list_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        crate::app_core::apply_blocks_schema_for_tests(&conn);
        let store = BlockStore::new(conn);
        let n = store
            .sync_note_blocks("n1", Some("ws"), "# A\n\nhello\n- [ ] t1")
            .unwrap();
        assert_eq!(n, 3);
        let blocks = store.list_note_blocks("n1").unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].block_type, "heading");
        assert_eq!(blocks[2].block_type, "task");
        // position 递增
        assert!(blocks[0].position < blocks[1].position);

        // 二次同步（内容变化）→ 软删旧 + 插新，活跃集正确
        store.sync_note_blocks("n1", Some("ws"), "# B").unwrap();
        let blocks = store.list_note_blocks("n1").unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, "# B");
    }
}
