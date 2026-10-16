//! 双向链接 S1（DK-双向链接卡，Bravo 领地）——wiki 链接提取+链接边索引。
//!
//! **本切片范围**：core 解析+索引原语。命令层编排（保存时自动 reindex）
//! =S2，反链面板/图谱=S3，id 解析（`[[note-id]]`）=S2 随 UI 语法定。
//!
//! ## 语法面（v1 按标题解析）
//!
//! - `[[target]]` / `[[target|alias]]`——target=`|` 前段 trim（alias 仅展示）
//! - **忽略**：``` 围栏代码块（跨行，围栏行自身不参与提取）与行内 `` ` ``
//!   成对段内的 `[[]]`
//! - 空目标（`[[]]`/`[[ ]]`/`[[|alias]]`）跳过；**去重保序**（首现顺序）
//!
//! ## KV 键域（已登记 KV capability 设计稿保留前缀清单）
//!
//! - `links:out:{src_note_id}` = serde_json(Vec<String>)——出链标题集，
//!   **全量覆写**（删旧写新，幂等）
//! - `links:in:{target_title_b64}` = serde_json(HashSet<note_id>)——反链
//!   倒排，**增量维护**（读旧集→apply diff→写回；空集删键防膨胀）
//! - b64=urlsafe no-pad（标题含中文/斜杠安全；decode 非法 → NotFound 语义）
//!
//! ## 悬挂边语义（v1 惰性）
//!
//! 目标笔记删除/改名时**不做主动清扫**（S2 决策清扫策略）——倒排中
//! 残留 note_id 保留；查询侧（S2）按 existence 惰性校验，悬挂标记
//! stale 不展示。本模块仅保证：reindex 的 diff 语义精确（增删边即时
//! 反映），残留只在"无 reindex 钩子的删除路径"出现。

use crate::traits::kv_store::KVStore;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 链接边（索引载荷；src→target 标题）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkEdge {
    pub src_note_id: String,
    pub target_title: String,
}

/// KV 键域前缀（出链/倒排）。
pub const LINKS_OUT_PREFIX: &str = "links:out:";
pub const LINKS_IN_PREFIX: &str = "links:in:";

// ---------- b64 urlsafe no-pad（手写——避免新增依赖） ----------

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// urlsafe no-pad 编码（标题含中文/斜杠 KV 键安全）。
pub fn b64url_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64URL[(n >> 18 & 0x3F) as usize] as char);
        out.push(B64URL[(n >> 12 & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64URL[(n >> 6 & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64URL[(n & 0x3F) as usize] as char);
        }
    }
    out
}

/// urlsafe no-pad 解码（非法输入 → None——调用方 NotFound 语义）。
pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> { B64URL.iter().position(|&x| x == c).map(|p| p as u32) };
    let bytes: Vec<u8> = s.bytes().collect();
    if bytes.len() % 4 == 1 {
        return None; // 非法长度
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            return None;
        }
        let mut n = val(chunk[0])? << 18 | val(chunk[1])? << 12;
        if chunk.len() > 2 {
            n |= val(chunk[2])? << 6;
        }
        if chunk.len() > 3 {
            n |= val(chunk[3])?;
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

// ---------- wiki 链接提取 ----------

/// 从 markdown 正文提取 wiki 链接目标（去重保序）。
///
/// 语法面见模块文档：`[[]]`/`[[|]]` 双形态；代码块围栏与行内码内忽略。
pub fn extract_wiki_links(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue; // 围栏行自身不参与
        }
        if in_fence {
            continue;
        }
        extract_line_links(line, &mut out);
    }
    out
}

/// 单行提取：先剥行内码段（成对 ` 之间），再扫 `[[...]]`。
fn extract_line_links(line: &str, out: &mut Vec<String>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                // 行内码：跳到下一个 `（不成对则视为普通字符）
                match line[i + 1..].find('`') {
                    Some(end) => i += 1 + end + 1,
                    None => i += 1,
                }
            }
            b'[' if bytes.get(i + 1) == Some(&b'[') => {
                // wiki 链接起：找闭合 ]]
                match line[i + 2..].find("]]") {
                    Some(end) => {
                        let inner = &line[i + 2..i + 2 + end];
                        let target = inner.split('|').next().unwrap_or("").trim();
                        if !target.is_empty() && !out.iter().any(|t| t == target) {
                            out.push(target.to_string());
                        }
                        i += 2 + end + 2;
                    }
                    None => i += 2, // 未闭合：跳过 [[ 继续扫
                }
            }
            _ => i += 1,
        }
    }
}

// ---------- 索引原语 ----------

/// 出链集读取（键缺失 = 空集）。
async fn read_out_links(kv: &dyn KVStore, note_id: &str) -> Result<Vec<String>, crate::Error> {
    match kv.get(&format!("{LINKS_OUT_PREFIX}{note_id}")).await? {
        Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
        None => Ok(Vec::new()),
    }
}

/// 倒排集读取（键缺失 = 空集）。
async fn read_in_links(
    kv: &dyn KVStore,
    target_title: &str,
) -> Result<HashSet<String>, crate::Error> {
    let key = format!(
        "{LINKS_IN_PREFIX}{}",
        b64url_encode(target_title.as_bytes())
    );
    match kv.get(&key).await? {
        Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
        None => Ok(HashSet::new()),
    }
}

/// 笔记链接索引重建：出链全量覆写 + 倒排增量 diff。
///
/// 调用方编排（S2 写路径钩子）：保存时传 (old_content, new_content)。
/// 首次建索可传 old_content=""（全量视为新增）。
///
/// # Errors
/// KV 读写/序列化失败（core Error 直传，无跨 crate 转接）。
pub async fn reindex_note_links(
    kv: &dyn KVStore,
    note_id: &str,
    old_content: &str,
    new_content: &str,
) -> Result<(), crate::Error> {
    let old_out = extract_wiki_links(old_content);
    let new_out = extract_wiki_links(new_content);

    // 出链：全量覆写（删旧写新——空集也写，幂等语义清晰）
    kv.set(
        &format!("{LINKS_OUT_PREFIX}{note_id}"),
        serde_json::to_vec(&new_out)?.as_slice(),
    )
    .await?;

    // 倒排：增量 diff（added 进集 / removed 出集——集合语义不比数组序）
    let old_set: HashSet<&String> = old_out.iter().collect();
    let new_set: HashSet<&String> = new_out.iter().collect();

    for target in new_set.difference(&old_set) {
        let key = format!("{LINKS_IN_PREFIX}{}", b64url_encode(target.as_bytes()));
        let mut set = read_in_links(kv, target).await?;
        set.insert(note_id.to_string());
        kv.set(&key, serde_json::to_vec(&set)?.as_slice()).await?;
    }
    for target in old_set.difference(&new_set) {
        let key = format!("{LINKS_IN_PREFIX}{}", b64url_encode(target.as_bytes()));
        let mut set = read_in_links(kv, target).await?;
        set.remove(note_id);
        if set.is_empty() {
            kv.delete(&key).await?; // 空集删键防膨胀
        } else {
            kv.set(&key, serde_json::to_vec(&set)?.as_slice()).await?;
        }
    }
    Ok(())
}

/// 反链查询（S2 查询侧原语——悬挂惰性校验的读入口）。
///
/// 返回指向 `target_title` 的 note_id 集。**悬挂语义**：集内 note_id
/// 可能已不存在（无 reindex 钩子的删除路径），调用方（S2）按笔记
/// existence 过滤，悬挂标记 stale 不展示。
pub async fn backlinks_of(
    kv: &dyn KVStore,
    target_title: &str,
) -> Result<HashSet<String>, crate::Error> {
    read_in_links(kv, target_title).await
}

/// 出链查询（S2 原语——note_id 无出链 = 空集）。
pub async fn outlinks_of(kv: &dyn KVStore, note_id: &str) -> Result<Vec<String>, crate::Error> {
    read_out_links(kv, note_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// 内存 KV mock（async_trait 与生产 trait 同签名——installer::tests 惯例）。
    struct MemKv(Mutex<HashMap<String, Vec<u8>>>);
    #[async_trait::async_trait]
    impl KVStore for MemKv {
        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, crate::Error> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        async fn set(&self, key: &str, value: &[u8]) -> Result<(), crate::Error> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_vec());
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<(), crate::Error> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
        async fn exists(&self, key: &str) -> Result<bool, crate::Error> {
            Ok(self.0.lock().unwrap().contains_key(key))
        }
        async fn batch_get(&self, keys: &[&str]) -> Result<Vec<Option<Vec<u8>>>, crate::Error> {
            let m = self.0.lock().unwrap();
            Ok(keys.iter().map(|k| m.get(*k).cloned()).collect())
        }
        async fn batch_set(&self, items: &[(&str, &[u8])]) -> Result<(), crate::Error> {
            let mut m = self.0.lock().unwrap();
            for (k, v) in items {
                m.insert((*k).to_string(), v.to_vec());
            }
            Ok(())
        }
        async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, crate::Error> {
            let m = self.0.lock().unwrap();
            Ok(m.iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
    }

    #[test]
    fn test_extract_basic_dedup_preserve_order() {
        let c = "see [[Alpha]] then [[Beta]] and [[Alpha]] again";
        assert_eq!(extract_wiki_links(c), vec!["Alpha", "Beta"]);
    }

    #[test]
    fn test_extract_alias_target_only() {
        let c = "[[Real Target|显示别名]] and [[Second]]";
        assert_eq!(extract_wiki_links(c), vec!["Real Target", "Second"]);
    }

    #[test]
    fn test_extract_code_fence_ignored() {
        let c = "[[Real]]\n```rust\nlet s = \"[[Fake]]\";\n```\n[[Also Real]]";
        assert_eq!(extract_wiki_links(c), vec!["Real", "Also Real"]);
    }

    #[test]
    fn test_extract_inline_code_ignored() {
        let c = "use `[[Not A Link]]` syntax but [[Real]]";
        assert_eq!(extract_wiki_links(c), vec!["Real"]);
    }

    #[test]
    fn test_extract_chinese_title() {
        let c = "看 [[ daily notes 与中文标题 ]] 这个";
        assert_eq!(extract_wiki_links(c), vec!["daily notes 与中文标题"]);
    }

    #[test]
    fn test_extract_empty_and_unclosed_skipped() {
        let c = "[[]] [[ ]] [[|alias]] [[unclosed";
        assert!(extract_wiki_links(c).is_empty());
    }

    #[test]
    fn test_b64url_chinese_roundtrip() {
        let title = "中文/标题 with space";
        let enc = b64url_encode(title.as_bytes());
        assert!(!enc.contains('=') && !enc.contains('/') && !enc.contains('+'));
        assert_eq!(b64url_decode(&enc).as_deref(), Some(title.as_bytes()));
        assert!(b64url_decode("!!!").is_none(), "非法 b64 → None");
    }

    #[tokio::test]
    async fn test_reindex_outlinks_full_overwrite() {
        let kv = MemKv(Mutex::new(HashMap::new()));
        reindex_note_links(&kv, "a", "", "old [[X]] [[Y]]")
            .await
            .unwrap();
        assert_eq!(outlinks_of(&kv, "a").await.unwrap(), vec!["X", "Y"]);
        // 删链后出链集收敛（全量覆写——旧键内容被替换）
        reindex_note_links(&kv, "a", "old [[X]] [[Y]]", "now [[Y]] [[Z]]")
            .await
            .unwrap();
        assert_eq!(outlinks_of(&kv, "a").await.unwrap(), vec!["Y", "Z"]);
    }

    #[tokio::test]
    async fn test_reindex_inverted_add_diff() {
        let kv = MemKv(Mutex::new(HashMap::new()));
        reindex_note_links(&kv, "a", "", "[[T1]] [[T2]]")
            .await
            .unwrap();
        reindex_note_links(&kv, "b", "", "[[T2]] [[T3]]")
            .await
            .unwrap();
        assert_eq!(
            backlinks_of(&kv, "T2").await.unwrap(),
            HashSet::from(["a".to_string(), "b".to_string()])
        );
        assert_eq!(
            backlinks_of(&kv, "T1").await.unwrap(),
            HashSet::from(["a".to_string()])
        );
    }

    #[tokio::test]
    async fn test_reindex_inverted_remove_diff_and_empty_key_prune() {
        let kv = MemKv(Mutex::new(HashMap::new()));
        reindex_note_links(&kv, "a", "", "[[T1]] [[T2]]")
            .await
            .unwrap();
        reindex_note_links(&kv, "b", "", "[[T1]]").await.unwrap();
        // a 撤销 T1 链 → 倒排只余 b；T2 空集键被删
        reindex_note_links(&kv, "a", "x [[T1]] [[T2]]", "y")
            .await
            .unwrap();
        assert_eq!(
            backlinks_of(&kv, "T1").await.unwrap(),
            HashSet::from(["b".to_string()])
        );
        assert!(
            kv.get(&format!(
                "{LINKS_IN_PREFIX}{}",
                b64url_encode("T2".as_bytes())
            ))
            .await
            .unwrap()
            .is_none(),
            "空集键删除"
        );
    }

    #[tokio::test]
    async fn test_hanging_edge_lazy_semantics() {
        let kv = MemKv(Mutex::new(HashMap::new()));
        reindex_note_links(&kv, "b", "", "[[X]]").await.unwrap();
        // 模拟无 reindex 钩子的删除路径：直接清 b 的出链键
        kv.delete(&format!("{LINKS_OUT_PREFIX}b")).await.unwrap();
        // v1 悬挂边保留（不主动清扫）——查询侧 S2 负责惰性过滤
        assert_eq!(
            backlinks_of(&kv, "X").await.unwrap(),
            HashSet::from(["b".to_string()])
        );
    }

    #[tokio::test]
    async fn test_decode_invalid_key_notfound_semantics() {
        let kv = MemKv(Mutex::new(HashMap::new()));
        // 非法 b64 键名 → 读不到 → 空集（NotFound 语义）
        let bad = "links:in:%not-b64%";
        assert!(kv.get(bad).await.unwrap().is_none());
    }
}
