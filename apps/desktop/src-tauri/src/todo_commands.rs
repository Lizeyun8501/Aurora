//! DK-2.2 待办聚合（v1 只读）：全库 task 块跨笔记聚合——审阅报告 P1 功能。
//!
//! 数据链：BlockStore.list_blocks_by_type("task")（core 原语，单条 SQL）
//! → 逐块解析 done 标记（`- [x]`/`- [X]` 前缀）→ note 标题补全（title
//! 恒明文通路）→ trash 排除。回查笔记跳转 onSelect 复用路由链。
//!
//! v1 边界：只读聚合（checkbox 切换回写=改原笔记内容——块定位+回写
//! 复杂度延后 v2）；删除标记由 blocks is_deleted=0 过滤兜底（内容删除
//! 后双写同步消失）。

use crate::{get_core, get_vault, unwrap_note_bytes};
use serde::Serialize;
use std::collections::HashSet;

/// 聚合任务项 — text 已剥 `- [ ] ` 前缀。
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub note_id: String,
    pub note_title: Option<String>,
    pub done: bool,
    pub text: String,
}

/// task 行解析：`- [x] `/`- [X] ` → (done=true, rest)；`- [ ] ` →
/// (false, rest)；非任务行 → None。
pub fn parse_task_line(content: &str) -> Option<(bool, String)> {
    let lower_prefix = "- [x] ";
    if let Some(rest) = content.strip_prefix(lower_prefix) {
        return Some((true, rest.to_string()));
    }
    if let Some(rest) = content.strip_prefix("- [X] ") {
        return Some((true, rest.to_string()));
    }
    if let Some(rest) = content.strip_prefix("- [ ] ") {
        return Some((false, rest.to_string()));
    }
    None
}

/// 全库待办聚合（2.2 v1 只读）。
///
/// # Errors
/// 应用未初始化 / KV 或 blocks 读失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_all_tasks() -> Result<Vec<TaskItem>, String> {
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

    // note_id → title（trash 排除；title 恒明文——锁定篇可读）
    let mut title_map: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
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
        let title = match unwrap_note_bytes(&core, &vault, &bytes) {
            Ok(json) => json
                .get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string()),
            Err(_) => None,
        };
        title_map.insert(id, title);
    }

    let Some(blocks) = crate::blocks_store() else {
        return Ok(Vec::new()); // 无 blocks 后端（内存降级）——空聚合
    };
    let rows = blocks
        .list_blocks_by_type("task")
        .map_err(|e| e.to_string())?;

    let mut items: Vec<TaskItem> = rows
        .into_iter()
        .filter_map(|(note_id, rec)| {
            if trash_ids.contains(&note_id) {
                return None;
            }
            let (done, text) = parse_task_line(&rec.content)?;
            Some(TaskItem {
                note_title: title_map.get(&note_id).cloned().flatten(),
                note_id,
                done,
                text,
            })
        })
        .collect();
    // 未完成在前，组内按笔记 id 稳定序
    items.sort_by(|a, b| a.done.cmp(&b.done).then_with(|| a.note_id.cmp(&b.note_id)));
    Ok(items)
}

#[cfg(test)]
mod todo_tests {
    use super::*;

    #[test]
    fn parse_task_variants() {
        assert_eq!(
            parse_task_line("- [ ] 买菜"),
            Some((false, "买菜".to_string()))
        );
        assert_eq!(
            parse_task_line("- [x] 已办"),
            Some((true, "已办".to_string()))
        );
        assert_eq!(
            parse_task_line("- [X] 大写也认"),
            Some((true, "大写也认".to_string()))
        );
        assert_eq!(parse_task_line("- [x]无空格"), None, "空格缺失不误判");
        assert_eq!(parse_task_line("- [ ] "), Some((false, String::new())));
        assert_eq!(parse_task_line("普通行"), None);
        assert_eq!(parse_task_line("## 标题"), None);
    }
}
