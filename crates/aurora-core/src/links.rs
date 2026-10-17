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

// ---------- wiki 链接改写（rename 跟随 — 拍板 2 A 方案） ----------

/// 正文 wiki 链接改写：target 段精确等于 `old` 的 `[[old]]`/`[[old|alias]]`
/// → `[[new]]`/`[[new|alias]]`（alias 保留）。
///
/// 语法面与 [`extract_wiki_links`] 同款状态机（围栏/行内码内不改写），
/// 逐行重写、非命中行原样返回（零拷贝语义：无命中返回 None）。
///
/// 返回 `(改写后正文, 命中数)`；`None` = 无命中（调用方跳过保存）。
pub fn rewrite_wiki_links(content: &str, old: &str, new: &str) -> Option<(String, usize)> {
    let mut buf = String::with_capacity(content.len() + new.len());
    let mut in_fence = false;
    let mut hits = 0usize;
    let mut changed = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            buf.push_str(line);
            buf.push('\n');
            continue;
        }
        if in_fence {
            buf.push_str(line);
            buf.push('\n');
            continue;
        }
        let (newline, line_hits) = rewrite_line_links(line, old, new);
        hits += line_hits;
        if line_hits > 0 {
            changed = true;
        }
        buf.push_str(&newline);
        buf.push('\n');
    }
    // 与 content.lines() 对齐：末尾无换行的原文不补 \n（去掉最后多加的）
    if !content.ends_with('\n') {
        buf.truncate(buf.trim_end_matches('\n').len());
    }
    if changed {
        Some((buf, hits))
    } else {
        None
    }
}

/// 单行改写：target 段精确匹配 `old` 才替换（alias 段原样保留）。
/// 返回 `(改写后行, 命中数)`。
fn rewrite_line_links(line: &str, old: &str, new: &str) -> (String, usize) {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut hits = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => match line[i + 1..].find('`') {
                Some(end) => {
                    let stop = i + 1 + end + 1;
                    out.push_str(&line[i..stop.min(line.len())]);
                    i = stop;
                }
                None => {
                    out.push_str(&line[i..]);
                    i = line.len();
                }
            },
            b'[' if bytes.get(i + 1) == Some(&b'[') => match line[i + 2..].find("]]") {
                Some(end) => {
                    let inner = &line[i + 2..i + 2 + end];
                    let target = inner.split('|').next().unwrap_or("").trim();
                    if target == old {
                        let alias = inner
                            .split_once('|')
                            .map(|(_, a)| format!("|{a}"))
                            .unwrap_or_default();
                        out.push_str(&format!("[[{new}{alias}]]"));
                        hits += 1;
                    } else {
                        out.push_str(&line[i..i + 2 + end + 2]);
                    }
                    i += 2 + end + 2;
                }
                None => {
                    out.push_str(&line[i..i + 2]);
                    i += 2;
                }
            },
            _ => {
                // UTF-8 安全步进：按字符边界推进
                let ch_len = line[i..].chars().next().map_or(1, char::len_utf8);
                out.push_str(&line[i..i + ch_len]);
                i += ch_len;
            }
        }
    }
    (out, hits)
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

/// 笔记链接索引重建 — **from_index 变体**（S2 复核拍板 1：旧集=KV
/// `links:out` 权威读，无需 old_content 明文——锁定态/不可读态编排友好）。
///
/// 与 [`reindex_note_links`] 唯一差异：旧出链集来源=KV 权威读（缺省空集
/// =首建语义），而非解析 old_content 明文。save 写路径接入用本变体。
///
/// # Errors
/// KV 读写/序列化失败。
pub async fn reindex_note_links_from_index(
    kv: &dyn KVStore,
    note_id: &str,
    new_content: &str,
) -> Result<(), crate::Error> {
    let old_out = read_out_links(kv, note_id).await.unwrap_or_default();
    let new_out = extract_wiki_links(new_content);
    apply_link_diff(kv, note_id, &old_out, &new_out).await
}

/// 笔记链接索引清除（S2 复核拍板 3：purge_note 物理删级联）。
///
/// 出链读 → 逐目标倒排 remove（空集删键）→ 删 `links:out:{note_id}`。
/// 调用方（purge_note）按附件级联先例编排：失败不阻塞 purge，日志留痕
/// （最终一致——残余悬挂由 S1 惰性语义兜底）。
///
/// # Errors
/// KV 读写/序列化失败。
pub async fn purge_note_links(kv: &dyn KVStore, note_id: &str) -> Result<(), crate::Error> {
    let old_out = read_out_links(kv, note_id).await.unwrap_or_default();
    for target in old_out {
        let key = format!("{LINKS_IN_PREFIX}{}", b64url_encode(target.as_bytes()));
        let mut set = read_in_links(kv, &target).await?;
        set.remove(note_id);
        if set.is_empty() {
            kv.delete(&key).await?;
        } else {
            kv.set(&key, serde_json::to_vec(&set)?.as_slice()).await?;
        }
    }
    kv.delete(&format!("{LINKS_OUT_PREFIX}{note_id}")).await?;
    Ok(())
}

/// diff 核心（两 reindex 变体共享）：出链覆写 + 倒排增删。
async fn apply_link_diff(
    kv: &dyn KVStore,
    note_id: &str,
    old_out: &[String],
    new_out: &[String],
) -> Result<(), crate::Error> {
    kv.set(
        &format!("{LINKS_OUT_PREFIX}{note_id}"),
        serde_json::to_vec(&new_out.to_vec())?.as_slice(),
    )
    .await?;

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
            kv.delete(&key).await?;
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

    // ---------- rewrite_wiki_links（rename 跟随 — 拍板 2 A 方案） ----------

    #[test]
    fn test_rewrite_basic_and_no_hit() {
        // 命中改写；无命中返回 None（调用方跳过保存——幂等基础）
        let (r, n) =
            rewrite_wiki_links("见 [[旧标题]] 和 [[其他]]", "旧标题", "新标题").expect("应命中");
        assert_eq!(n, 1);
        assert_eq!(r, "见 [[新标题]] 和 [[其他]]");
        assert!(rewrite_wiki_links("见 [[其他]]", "旧标题", "新标题").is_none());
    }

    #[test]
    fn test_rewrite_alias_preserved() {
        let (r, n) = rewrite_wiki_links("[[旧标题|我的别名]]", "旧标题", "新标题").expect("应命中");
        assert_eq!(n, 1);
        assert_eq!(r, "[[新标题|我的别名]]", "alias 段保留");
    }

    #[test]
    fn test_rewrite_exact_match_only() {
        // 前缀相似/含子串不改（精确匹配语义）
        let c = "[[旧标题续集]] [[前缀旧标题]] [[旧标题x]] [[旧标题]]";
        let (r, n) = rewrite_wiki_links(c, "旧标题", "新标题").expect("应命中 1");
        assert_eq!(n, 1, "仅精确命中一处");
        assert!(r.contains("[[旧标题续集]]"));
        assert!(r.contains("[[前缀旧标题]]"));
        assert!(r.contains("[[新标题]]"));
    }

    #[test]
    fn test_rewrite_fence_and_inline_code_immune() {
        // 围栏与行内码内不改写（与 extract 同款状态机）
        let c = "```\n[[旧标题]]\n```\n正文 [[旧标题]]\n`[[旧标题]]` 码内";
        let (r, n) = rewrite_wiki_links(c, "旧标题", "新标题").expect("应命中 1");
        assert_eq!(n, 1);
        assert!(r.contains("```\n[[旧标题]]\n```"), "围栏内原样");
        assert!(r.contains("`[[旧标题]]`"), "行内码内原样");
        assert!(r.contains("正文 [[新标题]]"));
    }

    #[test]
    fn test_rewrite_idempotent() {
        // 幂等：改写后再跑同参数 → None（follow 重跑安全基础）
        let once = rewrite_wiki_links("a [[旧标题]] b", "旧标题", "新标题")
            .expect("首次应命中")
            .0;
        assert!(
            rewrite_wiki_links(&once, "旧标题", "新标题").is_none(),
            "重跑无命中"
        );
    }

    #[test]
    fn test_rewrite_utf8_and_unclosed() {
        // UTF-8 边界步进安全
        let c = "中文正文 [[旧标题]] 尾";
        let (r, _) = rewrite_wiki_links(c, "旧标题", "新标题").expect("应命中");
        assert_eq!(r, "中文正文 [[新标题]] 尾");
        // 未闭合嵌套：extract 同款贪婪语义——首 [[ 到首 ]] 全段一个 target
        // （"旧标题 未闭合 [[旧标题" ≠ old）→ 无命中原样（与提取面一致）
        let c2 = "[[旧标题 未闭合 [[旧标题]] 正常";
        assert!(rewrite_wiki_links(c2, "旧标题", "新标题").is_none());
        // 未闭合=行内无 ]]（跨行重扫）：下一行独立命中
        let c3 = "[[unclosed 行内无闭合\n下一行 [[旧标题]] 正常";
        let (r3, n3) = rewrite_wiki_links(c3, "旧标题", "新标题").expect("应命中 1");
        assert_eq!(n3, 1);
        assert!(r3.contains("[[新标题]] 正常"));
        assert!(r3.contains("[[unclosed 行内无闭合"));
    }

    #[test]
    fn test_rewrite_trailing_newline_preserved() {
        // 末尾换行保真（lines() 重建不吞尾巴）
        let with_nl = "a [[旧标题]]\n";
        let (r, _) = rewrite_wiki_links(with_nl, "旧标题", "新标题").unwrap();
        assert_eq!(r, "a [[新标题]]\n");
        let no_nl = "a [[旧标题]]";
        let (r2, _) = rewrite_wiki_links(no_nl, "旧标题", "新标题").unwrap();
        assert_eq!(r2, "a [[新标题]]");
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
    async fn test_reindex_from_index_variant_authoritative_old() {
        let kv = MemKv(Mutex::new(HashMap::new()));
        // 预置旧出链（KV 权威——不依赖 old_content 明文）
        kv.set(
            &format!("{LINKS_OUT_PREFIX}n1"),
            serde_json::to_vec(&vec!["Old".to_string(), "Keep".to_string()])
                .unwrap()
                .as_slice(),
        )
        .await
        .unwrap();
        kv.set(
            &format!("{LINKS_IN_PREFIX}{}", b64url_encode(b"Old")),
            serde_json::to_vec(&HashSet::from(["n1".to_string()]))
                .unwrap()
                .as_slice(),
        )
        .await
        .unwrap();
        // Keep 旧倒排也预置（一致态——diff 不触应原样保留）
        kv.set(
            &format!("{LINKS_IN_PREFIX}{}", b64url_encode(b"Keep")),
            serde_json::to_vec(&HashSet::from(["n1".to_string()]))
                .unwrap()
                .as_slice(),
        )
        .await
        .unwrap();
        // from_index 变体：new 明文不含 Old（removed）含 Keep/New（Keep 不动/New added）
        reindex_note_links_from_index(&kv, "n1", "[[Keep]] [[New]]")
            .await
            .unwrap();
        let out = outlinks_of(&kv, "n1").await.unwrap();
        assert_eq!(out, vec!["Keep".to_string(), "New".to_string()]);
        assert!(
            backlinks_of(&kv, "Old").await.unwrap().is_empty(),
            "removed 出倒排（空集删键语义）"
        );
        assert!(backlinks_of(&kv, "Keep").await.unwrap().contains("n1"));
        assert!(backlinks_of(&kv, "New").await.unwrap().contains("n1"));
    }

    #[tokio::test]
    async fn test_purge_note_links_cascades() {
        let kv = MemKv(Mutex::new(HashMap::new()));
        // b 出链指向 X/Y；a 也指向 X（倒排残留语义）
        reindex_note_links(&kv, "b", "", "[[X]] [[Y]]")
            .await
            .unwrap();
        reindex_note_links(&kv, "a", "", "[[X]]").await.unwrap();
        purge_note_links(&kv, "b").await.unwrap();
        // b 出链键删除
        assert!(
            kv.get(&format!("{LINKS_OUT_PREFIX}b"))
                .await
                .unwrap()
                .is_none(),
            "out 键物理删"
        );
        // 倒排：X 仍含 a（不被误清）、Y 空集删键
        assert_eq!(
            backlinks_of(&kv, "X").await.unwrap(),
            HashSet::from(["a".to_string()])
        );
        assert!(backlinks_of(&kv, "Y").await.unwrap().is_empty());
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
