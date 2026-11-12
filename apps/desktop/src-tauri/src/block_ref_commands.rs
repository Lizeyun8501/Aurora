//! A-05（TA 2026-11-12 工单）: 块引用解析 + 面包屑 — 桌面命令层。
//!
//! # 设计
//! - `cmd_resolve_block_ref`: 单点读取目标笔记（**零全库扫描**——万级库
//!   不卡顿语义;unseal 走 `import_ctx` 同源通道,锁定 fail-closed 分流
//!   「锁定」态≠「失效」态——锁定可恢复,失效=源已删）
//! - `cmd_breadcrumb`: core `block_ref::breadcrumb_of` 唯一递归源（双端
//!   共用 VM 字段 {id,title}[]——禁双端各写一套）
//! - 保存路径 emit: `aurora:doc-updated` Tauri 事件（多窗口 A-10 场景
//!   下引用卡 1s 内增量刷新——EventBus Medium 通道为唯一消费者投影环
//!   路（契约冻结）,文档内容事件无 Low 通道位;v1 走保存路径广播桥,
//!   同等语义,偏差披露于交付报告）

use serde::Serialize;

/// 引用解析结果（三态分流——前端嵌入卡直接消费）。
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BlockRefResolution {
    Ok {
        title: String,
        section_md: String,
    },
    NotFound,
    /// Private 锁定/解封失败——与 NotFound 分流（可恢复态,不误报失效）。
    Locked,
}

/// 解析块引用（`((noteId))` / `((noteId#anchor))` 的后端面）。
///
/// # Errors
/// id 非法/存储底层错误（IPC 文案透传）。
#[tauri::command]
pub async fn cmd_resolve_block_ref(
    note_id: String,
    anchor: Option<String>,
) -> Result<BlockRefResolution, String> {
    if !valid_note_id(&note_id) {
        return Err("非法笔记 id".into());
    }
    let core = super::get_core()?;
    let vault = super::get_vault()?;
    let key = format!("note:{note_id}");
    let Some(data) = core.kv_store.get(&key).await.map_err(|e| e.to_string())? else {
        return Ok(BlockRefResolution::NotFound);
    };
    // unseal 同源通道（backlink 样板:unwrap_note_bytes 含 legacy 明文回退;
    // 解密失败=锁定/不可读 → Locked 分流,与 NotFound 语义分离）
    let json = match super::unwrap_note_bytes(&core, &vault, &data) {
        Ok(v) => v,
        Err(_) => return Ok(BlockRefResolution::Locked),
    };
    let meta_title = json
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("未命名")
        .to_string();
    let content = json
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    match aurora_core::l3_domain::block_ref::resolve_section(&content, anchor.as_deref()) {
        aurora_core::l3_domain::block_ref::SectionResolve::Ok { title, section_md } => {
            // 元数据 title 优先（内容首行提取仅回退）
            Ok(BlockRefResolution::Ok {
                title: if meta_title.is_empty() {
                    title
                } else {
                    meta_title
                },
                section_md,
            })
        }
        aurora_core::l3_domain::block_ref::SectionResolve::AnchorNotFound { .. } => {
            Ok(BlockRefResolution::Ok {
                title: meta_title,
                section_md: "（引用锚点未命中——源笔记存在,标题锚点已变更）".into(),
            })
        }
    }
}

/// 面包屑（双端共用 VM——core 单一递归源）。
///
/// # Errors
/// 存储底层错误透传;目标笔记缺失 → 空链（宽容,非报错）。
#[tauri::command]
pub async fn cmd_breadcrumb(
    note_id: String,
) -> Result<Vec<aurora_core::l3_domain::block_ref::Crumb>, String> {
    if !valid_note_id(&note_id) {
        return Err("非法笔记 id".into());
    }
    let core = super::get_core()?;
    aurora_core::l3_domain::block_ref::breadcrumb_of(core.kv_store.as_ref(), &note_id)
        .await
        .map_err(|e| e.to_string())
}

/// 笔记 id 白名单（注入防护——note:{id} 键拼接;与 backlink 命令同款语义）。
fn valid_note_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// base64 引擎引用（防 unused import 误删——保留给后续分片扩展）。
#[allow(dead_code)]
fn _b64_keep() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}
