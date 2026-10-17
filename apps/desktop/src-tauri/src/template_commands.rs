//! DK-2.4 通用模板系统（v1）：模板即笔记——`template` 标签约定。
//!
//! 设计裁定：零新数据模型——笔记 tags 含 `template` 即为模板（Obsidian
//! 惯例轻量化）；daily note 模板面（DK-19）已有，本卡扩**任意笔记从模板
//! 创建**。占位符：{{date}}/{{weekday}}（复用 core render_daily_template）
//! + {{title}}/{{time}}（本层补替，core 不动）。
//!
//! v1 边界：无模板管理 UI（打标签即注册）；无变量自定义；创建后不自动
//! 打开（前端跳转路由已有）。

use crate::{get_core, get_vault, unwrap_note_bytes, ATTACH_STATE};
use serde::Serialize;
use std::collections::HashSet;

/// 模板项（列表面）。
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateItem {
    pub note_id: String,
    pub title: String,
}

/// 通用模板渲染：core daily 渲染（{{date}}/{{weekday}}）+ 本层补
/// {{title}}/{{time}}（HH:MM）。date 传当天（模板创建即时语义）。
pub fn render_general_template(template: &str, title: &str, date: &str, time: &str) -> String {
    let base = aurora_core::write_path::render_daily_template(template, date);
    base.replace("{{title}}", title).replace("{{time}}", time)
}

/// 全库模板列表：tags 含 `template` 的笔记（trash 排除；Folder/
/// SmartFolder 排除——模板域=笔记；损坏篇跳过非阻断）。
///
/// # Errors
/// 应用未初始化 / KV 扫描失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_list_templates() -> Result<Vec<TemplateItem>, String> {
    let core = get_core()?;
    let vault = get_vault()?;

    let trashed: HashSet<String> = core
        .kv_store
        .scan_prefix("trash:")
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(k, _)| k.strip_prefix("trash:").map(|s| s.to_string()))
        .collect();

    let mut out: Vec<TemplateItem> = Vec::new();
    for (key, bytes) in core
        .kv_store
        .scan_prefix("note:")
        .await
        .map_err(|e| e.to_string())?
    {
        let Some(id) = key.strip_prefix("note:").map(|s| s.to_string()) else {
            continue;
        };
        if trashed.contains(&id) {
            continue;
        }
        let Ok(json) = unwrap_note_bytes(&core, &vault, &bytes) else {
            continue; // 损坏/不可读跳过（非阻断）
        };
        let kind = json.get("kind").and_then(|k| k.as_str()).unwrap_or("Note");
        if kind == "Folder" || kind == "SmartFolder" {
            continue;
        }
        let is_template = json
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|a| a.iter().any(|v| v.as_str() == Some("template")))
            .unwrap_or(false);
        if !is_template {
            continue;
        }
        if let Some(t) = json.get("title").and_then(|t| t.as_str()) {
            if !t.is_empty() {
                out.push(TemplateItem {
                    note_id: id,
                    title: t.to_string(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(out)
}

/// 从模板创建新笔记：读模板明文 → 渲染占位符 → create_note +
/// save_note_content（全纪律：双写/blocks/links reindex 继承）。
///
/// 模板不可读（锁定/损坏）→ Err（创建无从谈起——fail-closed）。
///
/// # Errors
/// 应用未初始化 / 模板不存在或不可读 / KV 写失败（字符串化透传）。
#[tauri::command]
pub async fn cmd_create_from_template(
    template_id: String,
    title: String,
) -> Result<String, String> {
    if title.trim().is_empty() {
        return Err("标题不能为空".to_string());
    }
    let core = get_core()?;
    let vault = get_vault()?;

    // 模板明文（锁定/损坏 fail-closed——创建失败优于创建出空模板）
    let bytes = core
        .kv_store
        .get(&format!("note:{template_id}"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("模板不存在: {template_id}"))?;
    let json = unwrap_note_bytes(&core, &vault, &bytes)
        .map_err(|e| format!("模板不可读（锁定或损坏）: {e}"))?;
    let content = json
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let now = chrono::Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let time = now.format("%H:%M").to_string();
    let rendered = render_general_template(&content, &title, &date, &time);

    // WriteContext 组装（cmd_daily_note_open 同款密封闭包形态）
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
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
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    };

    let note_id = aurora_core::write_path::create_note(&ctx, &title)
        .await
        .map_err(|e| e.to_string())?;
    if !rendered.is_empty() {
        aurora_core::write_path::save_note_content(&ctx, &note_id, &rendered)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(note_id)
}

#[cfg(test)]
mod template_tests {
    use super::*;

    #[test]
    fn render_placeholders() {
        let t = "# {{title}}\n{{date}} {{weekday}} {{time}}\n";
        let r = render_general_template(t, "周会", "2026-10-17", "14:30");
        assert!(r.contains("# 周会"));
        assert!(r.contains("2026-10-17"));
        assert!(r.contains("星期六"));
        assert!(r.contains("14:30"));
        // 无占位符原样
        assert_eq!(
            render_general_template("纯文本", "x", "2026-10-17", "14:30"),
            "纯文本"
        );
    }
}
