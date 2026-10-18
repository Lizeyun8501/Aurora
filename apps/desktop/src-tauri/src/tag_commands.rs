//! DK-2.5 标签系统完善（v1）：全库标签聚合 + 组合过滤读取面——审阅报告 P2 功能。
//!
//! 数据链：KV `note:*` 逐条 unwrap_note_bytes（title 恒明文通路）→
//! NoteRecord.tags（DK-27 轻量读取源，Vec<String>）→ 聚合/过滤。
//! 写通路复用既有 cmd_set_note_tags（write_path::set_note_tags 全量
//! 覆盖语义+NoteMetadataChanged 事件投影联动）——本文件零新写面。
//!
//! v1 边界：分层标签（`a/b`）后端平铺返回，前端按 `/` 缩进呈现（树形
//! 折叠延后）；组合过滤 include=AND / exclude=NOT（FilterRule 同语义），
//! OR 语义延后；无标签索引（逐条 unwrap——全库量级内可接受，索引化挂起）。

use crate::{get_core, get_vault, unwrap_note_bytes};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// kind 字段是否计入标签面（Folder/SmartFolder 非内容载体——排除）。
fn kind_countable(kind: Option<&str>) -> bool {
    !matches!(kind, Some("Folder") | Some("SmartFolder"))
}

/// note json → tags（缺字段=空集；非数组=空集——脏数据不炸聚合）。
pub fn tags_of(json: &serde_json::Value) -> Vec<String> {
    json.get("tags")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 组合过滤判定（纯函数）：include 全含（AND，空=忽略）+ exclude 全无
/// （NOT，空=忽略）——FilterRule 同语义。
pub fn tags_match(note_tags: &[String], include: &[String], exclude: &[String]) -> bool {
    let set: HashSet<&String> = note_tags.iter().collect();
    include.iter().all(|t| set.contains(t)) && exclude.iter().all(|t| !set.contains(t))
}

/// 标签聚合计数项（count=含该标签的笔记数；tag 保留 `a/b` 原样——
/// 分层呈现归前端）。
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

/// 全库标签聚合（trash/Folder/SmartFolder 排除）。
///
/// # Errors
/// 应用未初始化 / KV 读失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_list_tags() -> Result<Vec<TagCount>, String> {
    let core = get_core()?;
    let vault = get_vault()?;

    let trash_ids: HashSet<String> = core
        .kv_store
        .scan_prefix("trash:")
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(k, _)| k.strip_prefix("trash:").map(|s| s.to_string()))
        .collect();

    let mut counts: HashMap<String, usize> = HashMap::new();
    for (k, bytes) in core
        .kv_store
        .scan_prefix("note:")
        .await
        .map_err(|e| e.to_string())?
    {
        let Some(id) = k.strip_prefix("note:").map(|s| s.to_string()) else {
            continue;
        };
        if trash_ids.contains(&id) {
            continue;
        }
        let Ok(json) = unwrap_note_bytes(&core, &vault, &bytes) else {
            continue; // 锁定/损坏——聚合面跳过（不阻断其余）
        };
        if !kind_countable(json.get("kind").and_then(|v| v.as_str())) {
            continue;
        }
        for t in tags_of(&json) {
            if t.is_empty() {
                continue;
            }
            *counts.entry(t).or_insert(0) += 1;
        }
    }
    let mut out: Vec<TagCount> = counts
        .into_iter()
        .map(|(tag, count)| TagCount { tag, count })
        .collect();
    // 计数降序，同数按标签字典序稳定
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
    Ok(out)
}

/// 组合过滤命中项（tags 全量回传——UI 展示+二次过滤零成本）。
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaggedNote {
    pub note_id: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
}

/// 按标签组合取笔记（include AND + exclude NOT；trash/Folder/SmartFolder
/// 排除；结果按标题字典序稳定——None 尾置）。
///
/// # Errors
/// 应用未初始化 / KV 读失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_notes_by_tags(
    include: Vec<String>,
    exclude: Vec<String>,
) -> Result<Vec<TaggedNote>, String> {
    let core = get_core()?;
    let vault = get_vault()?;

    let trash_ids: HashSet<String> = core
        .kv_store
        .scan_prefix("trash:")
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(k, _)| k.strip_prefix("trash:").map(|s| s.to_string()))
        .collect();

    let mut out: Vec<TaggedNote> = Vec::new();
    for (k, bytes) in core
        .kv_store
        .scan_prefix("note:")
        .await
        .map_err(|e| e.to_string())?
    {
        let Some(id) = k.strip_prefix("note:").map(|s| s.to_string()) else {
            continue;
        };
        if trash_ids.contains(&id) {
            continue;
        }
        let Ok(json) = unwrap_note_bytes(&core, &vault, &bytes) else {
            continue;
        };
        if !kind_countable(json.get("kind").and_then(|v| v.as_str())) {
            continue;
        }
        let tags = tags_of(&json);
        if tags_match(&tags, &include, &exclude) {
            out.push(TaggedNote {
                note_id: id,
                title: json
                    .get("title")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string()),
                tags,
            });
        }
    }
    out.sort_by(|a, b| {
        a.title
            .clone()
            .unwrap_or_default()
            .cmp(&b.title.clone().unwrap_or_default())
    });
    Ok(out)
}

#[cfg(test)]
mod tag_tests {
    use super::*;

    #[test]
    fn tags_of_variants() {
        assert_eq!(
            tags_of(&serde_json::json!({"tags": ["a", "a/b"]})),
            vec!["a".to_string(), "a/b".to_string()]
        );
        assert_eq!(tags_of(&serde_json::json!({})), Vec::<String>::new());
        assert_eq!(
            tags_of(&serde_json::json!({"tags": "脏数据"})),
            Vec::<String>::new()
        );
        assert_eq!(
            tags_of(&serde_json::json!({"tags": [1, "ok", null]})),
            vec!["ok".to_string()]
        );
    }

    #[test]
    fn match_combinations() {
        let tags = vec!["wip".to_string(), "a/b".to_string()];
        assert!(tags_match(&tags, &["wip"], &[]));
        assert!(tags_match(&tags, &["wip", "a/b"], &[])); // AND
        assert!(!tags_match(&tags, &["wip", "其他"], &[]));
        assert!(tags_match(&tags, &[], &["不存在"])); // NOT
        assert!(!tags_match(&tags, &[], &["a/b"]));
        assert!(tags_match(&tags, &[], &[])); // 全空=恒真
    }

    #[test]
    fn kind_gate() {
        assert!(kind_countable(None));
        assert!(kind_countable(Some("Note")));
        assert!(kind_countable(Some("DailyNote")));
        assert!(!kind_countable(Some("Folder")));
        assert!(!kind_countable(Some("SmartFolder")));
    }
}
