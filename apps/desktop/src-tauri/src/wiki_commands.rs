//! DK-编辑器补全：`[[` wiki 链接标题候选——全库标题聚合（补全数据层）。
//!
//! 领地：desktop/tauri（新文件）；core 不动（标题列表桌面侧聚合——
//! Alpha 派单纪律）。通路：unwrap_note_bytes（aab8997 先例——record 整包
//! vault 设备级 seal，笔记级锁定只加密 content 字段，title 恒明文可读）。
//! 域纪律：wiki 链接域=笔记标题——排除 Folder/SmartFolder（rename 跟随
//! 同款语义面）；trash 排除（trash:{id} 独立 marker 集对账）。

use crate::{get_core, get_vault, unwrap_note_bytes};
use std::collections::HashSet;

/// 全库笔记标题候选（补全数据层）。
///
/// 扫 `note:*` → unseal 读 meta title（锁定篇 title 明文通路）；空标题
/// 跳过；Folder/SmartFolder 排除；trash marker 集对账排除；去重排序。
/// 不可读篇（record 解析失败）跳过非阻断——补全缺一篇候选可接受。
///
/// # Errors
/// 应用未初始化 / KV 扫描失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_wiki_link_titles() -> Result<Vec<String>, String> {
    let core = get_core()?;
    let vault = get_vault()?;

    // trash 排除集（trash:{note_id} marker——list_trashed 同款键形）
    let trashed: HashSet<String> = core
        .kv_store
        .scan_prefix("trash:")
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(k, _)| k.strip_prefix("trash:").map(|s| s.to_string()))
        .collect();

    let mut titles: Vec<String> = Vec::new();
    for (key, bytes) in core
        .kv_store
        .scan_prefix("note:")
        .await
        .map_err(|e| e.to_string())?
    {
        let Some(id) = key.strip_prefix("note:") else {
            continue;
        };
        if trashed.contains(id) {
            continue;
        }
        // record 解析失败（损坏/不可读）跳过——非阻断（补全容忍缺候选）
        let Ok(json) = unwrap_note_bytes(&core, &vault, &bytes) else {
            continue;
        };
        let kind = json.get("kind").and_then(|k| k.as_str()).unwrap_or("Note");
        if kind == "Folder" || kind == "SmartFolder" {
            continue;
        }
        if let Some(t) = json.get("title").and_then(|t| t.as_str()) {
            if !t.is_empty() {
                titles.push(t.to_string());
            }
        }
    }
    titles.sort();
    titles.dedup();
    Ok(titles)
}
