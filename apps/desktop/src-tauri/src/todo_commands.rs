//! DK-2.2 待办聚合：全库 task 块跨笔记聚合 + checkbox 回写（v2）——审阅报告 P1 功能。
//!
//! 数据链：BlockStore.list_blocks_by_type("task")（core 原语，单条 SQL）
//! → 逐块解析 done 标记（`- [x]`/`- [X]` 前缀）→ note 标题补全（title
//! 恒明文通路）→ trash 排除。回查笔记跳转 onSelect 复用路由链。
//!
//! v2 回写：TaskItem 携 block 定位（block_id+position）——cmd_toggle_task
//! 读原文明文 → 按「第 idx 个 task 行 + 行内容恒等」双条件定位（同文本
//! 多行消歧）→ 行级切换 → save_note_content 全纪律（双写/blocks reindex：
//! 行内容变→块 id 随整篇 hash 变——前端聚合面用后即弃，下次拉取刷新）。
//!
//! 删除标记由 blocks is_deleted=0 过滤兜底（内容删除后双写同步消失）。

use crate::{get_core, get_vault, unwrap_note_bytes};
use serde::Serialize;
use std::collections::HashSet;

/// 聚合任务项 — text 已剥 `- [ ] ` 前缀；block_id/position 为回写定位键。
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub note_id: String,
    pub note_title: Option<String>,
    pub done: bool,
    pub text: String,
    /// 回写定位键：blocks 表主键（`blk_{hash}_{idx}`，整篇内容 hash——
    /// checkbox 切换后失效，仅当前聚合快照内有效）。
    pub block_id: String,
    /// 块序位（split_markdown_blocks 序 × 100.0）——同文本多行消歧键。
    pub block_position: f64,
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
                block_id: rec.id,
                block_position: rec.position,
            })
        })
        .collect();
    // 未完成在前，组内按笔记 id 稳定序
    items.sort_by(|a, b| a.done.cmp(&b.done).then_with(|| a.note_id.cmp(&b.note_id)));
    Ok(items)
}

/// 回写行定位（纯函数，确定性）：在原文明文中找「第 `task_idx` 个任务行
/// 且整行 == `block_content`」的目标行——同文本多行时按块序位消歧，
/// 双条件任一不满足 → None（调用方报错不盲写）。
///
/// 返回 Some((行号0-based, 当前行文本))。
pub fn locate_task_line(
    content: &str,
    block_content: &str,
    position: f64,
) -> Option<(usize, String)> {
    let idx = (position / 100.0).round() as usize;
    let mut task_seen: usize = 0;
    for (i, line) in content.lines().enumerate() {
        if parse_task_line(line.trim_start()).is_some() {
            if task_seen == idx && line == block_content {
                return Some((i, line.to_string()));
            }
            task_seen += 1;
        }
    }
    None
}

/// checkbox 状态切换（纯函数）：`- [ ] ` ⇄ `- [x] `（大写 X 归一小写，
/// 与 split/parse 语义一致）。非任务行 → None。
pub fn toggle_task_line(line: &str) -> Option<String> {
    if let Some(rest) = line.trim_start().strip_prefix("- [ ] ") {
        let indent = &line[..line.len() - line.trim_start().len()];
        return Some(format!("{indent}- [x] {rest}"));
    }
    if let Some(rest) = line.trim_start().strip_prefix("- [x] ") {
        let indent = &line[..line.len() - line.trim_start().len()];
        return Some(format!("{indent}- [ ] {rest}"));
    }
    if let Some(rest) = line.trim_start().strip_prefix("- [X] ") {
        let indent = &line[..line.len() - line.trim_start().len()];
        return Some(format!("{indent}- [ ] {rest}"));
    }
    None
}

/// 按 notes 前缀行重拼明文（lines() 丢失尾换行语义——保留原尾换行）。
fn rejoin_lines(content: &str, lines: &[&str]) -> String {
    let mut out = lines.join("\n");
    if content.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// DK-2.2 v2：checkbox 切换回写原笔记。定位=blocks 表 block_id 查活跃块
/// → position/100=块序 + 行内容恒等双条件 → 行级切换 → save_note_content
/// 全纪律（双写/blocks reindex/索引联动继承）。
///
/// # Errors
/// 应用未初始化 / 块不存在或已失配（聚合快照过期）/ KV 写失败
/// （字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_toggle_task(note_id: String, block_id: String, done: bool) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;

    // 块定位（活跃块中查 id——聚合快照过期/已删除 → 明确报错）
    let blocks = crate::blocks_store().ok_or("blocks 后端不可用（内存降级）")?;
    let recs = blocks
        .list_note_blocks(&note_id)
        .map_err(|e| e.to_string())?;
    let rec = recs
        .iter()
        .find(|r| r.id == block_id && !r.is_deleted)
        .ok_or_else(|| "任务块已失效（笔记可能已变更）——刷新聚合后重试".to_string())?;
    if rec.block_type != "task" {
        return Err("定位块非任务块——拒绝回写".to_string());
    }

    // 原文明文（锁定/损坏 fail-closed——回写无从谈起）
    let bytes = core
        .kv_store
        .get(&format!("note:{note_id}"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("笔记不存在: {note_id}"))?;
    let json = unwrap_note_bytes(&core, &vault, &bytes)
        .map_err(|e| format!("笔记不可读（锁定或损坏）: {e}"))?;
    let content = json
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    // 双条件定位（行内容恒等 + 块序消歧）
    let (line_no, line) = locate_task_line(&content, &rec.content, rec.position)
        .ok_or_else(|| "任务行定位失败（内容可能已变更）——刷新聚合后重试".to_string())?;
    let toggled = toggle_task_line(&line).ok_or("定位行非任务行——拒绝回写")?;
    // 目标态校验（前端乐观态与服务端一致的护栏）
    let now_done = parse_task_line(toggled.trim_start())
        .map(|(d, _)| d)
        .unwrap_or(false);
    if now_done != done {
        return Err("切换结果与目标状态不一致——刷新聚合后重试".to_string());
    }

    let mut lines: Vec<&str> = content.lines().collect();
    lines[line_no] = toggled.as_str();
    let new_content = rejoin_lines(&content, &lines);

    // 全纪律写回（WriteContext 同款——todo v1 命令区无现成 ctx 组装，就地组）
    let vault_seal = vault.clone();
    let crypto = core.crypto.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: crate::sync_commands::get_booted()?.core.clone(),
        blocks: crate::blocks_store(),
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None,
        attachments: crate::ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };
    aurora_core::write_path::save_note_content(&ctx, &note_id, &new_content)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
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

    #[test]
    fn toggle_line_variants() {
        assert_eq!(
            toggle_task_line("- [ ] 买菜"),
            Some("- [x] 买菜".to_string())
        );
        assert_eq!(
            toggle_task_line("- [x] 已办"),
            Some("- [ ] 已办".to_string())
        );
        assert_eq!(
            toggle_task_line("- [X] 大写归一"),
            Some("- [ ] 大写归一".to_string())
        );
        // 缩进保留
        assert_eq!(
            toggle_task_line("  - [ ] 缩进任务"),
            Some("  - [x] 缩进任务".to_string())
        );
        assert_eq!(toggle_task_line("普通行"), None);
    }

    #[test]
    fn locate_disambiguates_same_text() {
        let c = "# 日程\n- [ ] 站会\n- [x] 复盘\n- [ ] 站会\n";
        // 第 0 个 task（idx 0×100）+ 行恒等 → 行 1
        assert_eq!(
            locate_task_line(c, "- [ ] 站会", 0.0),
            Some((1, "- [ ] 站会".into()))
        );
        // 第 2 个 task（idx 2×100）同文本 → 行 3（消歧成功）
        assert_eq!(
            locate_task_line(c, "- [ ] 站会", 200.0),
            Some((3, "- [ ] 站会".into()))
        );
        // 行内容与块不符 → None（不盲写）
        assert_eq!(locate_task_line(c, "- [ ] 不存在", 0.0), None);
        // 序位越界 → None
        assert_eq!(locate_task_line(c, "- [ ] 站会", 300.0), None);
        // 尾换行保留语义：回拼不丢
        let (n, l) = locate_task_line(c, "- [x] 复盘", 100.0).unwrap();
        let mut lines: Vec<&str> = c.lines().collect();
        lines[n] = toggle_task_line(&l).unwrap().as_str();
        let out = rejoin_lines(c, &lines);
        assert!(out.ends_with('\n') && out.contains("- [ ] 复盘"));
    }
}
