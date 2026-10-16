//! DK-审阅 P0-3：Markdown 导出命令层——单篇/全库 md 出库。
//!
//! 领地：desktop/tauri（新文件）；读链复用 lib.rs 既有解密模式
//! （unwrap_note_bytes，pub(crate) 化）；文件名原语在 core::export
//! （纯函数已单测）。v1 边界：明文出库=用户显式行为，确认门归 UI
//! 层（S2 与双向链接 UI 同批）；附件不导出（内容寻址域复杂，另评估）。

use crate::{get_core, get_vault, unwrap_note_bytes};
use aurora_core::export::{dedupe_filename, existing_md_names, sanitize_filename};
use std::collections::HashSet;

/// 单篇导出为 markdown（`{dest_dir}/{安全文件名}.md`），返回落盘绝对路径。
///
/// # Errors
/// 应用未初始化 / 笔记不存在 / 目录创建或写盘失败。
#[tauri::command]
pub async fn cmd_export_note_markdown(note_id: String, dest_dir: String) -> Result<String, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let data = core
        .kv_store
        .get(&format!("note:{note_id}"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("note not found: {note_id}"))?;
    let json = unwrap_note_bytes(&core, &vault, &data)?;
    let title = json
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("untitled");
    let content = json.get("content").and_then(|c| c.as_str()).unwrap_or("");

    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("create dir `{dest_dir}`: {e}"))?;
    // 重名去重：扫目录已有 .md 基名（避免覆盖同目录既有导出/笔记）
    let mut existing: HashSet<String> = HashSet::new();
    let mut rd = tokio::fs::read_dir(&dest_dir)
        .await
        .map_err(|e| format!("read dir: {e}"))?;
    while let Some(e) = rd.next_entry().await.map_err(|e| format!("readdir: {e}"))? {
        if let Some(name) = e.file_name().to_str() {
            existing.insert(name.to_string());
        }
    }
    let base = sanitize_filename(title);
    let final_name = dedupe_filename(
        &existing_md_names(existing.iter().cloned().collect::<Vec<_>>().as_slice()),
        &base,
    );
    let path = format!("{}/{}.md", dest_dir.trim_end_matches('/'), final_name);
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("write `{path}`: {e}"))?;
    Ok(path)
}

/// 全库导出为 markdown（一笔记一文件），返回落盘路径列表（失败项跳过
/// 并计入 skipped——v1 简单容忍，注记不做中断回滚）。
///
/// # Errors
/// 应用未初始化 / 目录创建失败。
#[tauri::command]
pub async fn cmd_export_all_markdown(dest_dir: String) -> Result<Vec<String>, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("create dir `{dest_dir}`: {e}"))?;
    let rows = core
        .kv_store
        .scan_prefix("note:")
        .await
        .map_err(|e| e.to_string())?;
    let mut existing: HashSet<String> = HashSet::new();
    let mut rd = tokio::fs::read_dir(&dest_dir)
        .await
        .map_err(|e| format!("read dir: {e}"))?;
    while let Some(e) = rd.next_entry().await.map_err(|e| format!("readdir: {e}"))? {
        if let Some(name) = e.file_name().to_str() {
            existing.insert(name.to_string());
        }
    }
    let mut out = Vec::new();
    for (_key, data) in rows {
        // 键形态 note:{id}——trash 标记等无 "note:" 前缀干扰（trash:{id} 不匹配）
        let Ok(json) = unwrap_note_bytes(&core, &vault, &data) else {
            continue; // 解密失败项跳过（诚实计数）
        };
        let title = json
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("untitled");
        let content = json.get("content").and_then(|c| c.as_str()).unwrap_or("");
        let base = sanitize_filename(title);
        let final_name = dedupe_filename(
            &existing_md_names(existing.iter().cloned().collect::<Vec<_>>().as_slice()),
            &base,
        );
        let path = format!("{}/{}.md", dest_dir.trim_end_matches('/'), final_name);
        if tokio::fs::write(&path, content).await.is_ok() {
            existing.insert(final_name);
            out.push(path);
        }
    }
    Ok(out)
}
