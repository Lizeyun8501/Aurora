//! DK-UI-S2：反链面板数据层——S1 `backlinks_of` 读取侧首个消费方。
//!
//! 领地：desktop/tauri（新文件）；读链复用 unwrap_note_bytes（pub(crate)，
//! aab8997 先例）。锁定态纪律：反链项只出 note_id+引用方标题（标题读
//! meta 失败=锁定篇 → title=None，UI 显示 note_id——不触碰内容/无解密
//! 路径）；当前笔记锁定（unwrap fail-closed）→ 空集降级，UI 空态占位。

use crate::{get_core, get_vault, unwrap_note_bytes};
use aurora_core::links::backlinks_of;
use serde::Serialize;

/// 反链项 — title=None 表示引用方为锁定篇（标题不可读，仅知有引用）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklinkItem {
    pub note_id: String,
    pub title: Option<String>,
}

/// 反向链接列表（DK-UI-S2）。
///
/// note_id → 当前笔记标题（锁定态 unwrap 失败 → 空集降级，不阻断面板）
/// → backlinks_of（S1 原语，倒排 b64url 键内聚）→ 逐引用方读 meta 标题
/// （读不出 = 锁定篇 → title=None）。
///
/// # Errors
/// 应用未初始化 / KV 读失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_backlinks_of(note_id: String) -> Result<Vec<BacklinkItem>, String> {
    let core = get_core()?;
    let vault = get_vault()?;

    // 当前笔记标题（反链查询目标）——锁定篇 fail-closed → 空集降级
    let data = match core
        .kv_store
        .get(&format!("note:{note_id}"))
        .await
        .map_err(|e| e.to_string())?
    {
        Some(d) => d,
        None => return Ok(Vec::new()),
    };
    let title = match unwrap_note_bytes(&core, &vault, &data) {
        Ok(json) => json
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("untitled")
            .to_string(),
        Err(_) => return Ok(Vec::new()), // 锁定/不可读：空态降级（无解密路径）
    };

    // 倒排读（S1 原语）——失败同样降级空集（面板非阻断纪律）
    let ids = match backlinks_of(core.kv_store.as_ref(), &title).await {
        Ok(set) => set,
        Err(e) => {
            tracing::warn!(note_id = %note_id, error = %e, "backlinks query failed; degrade to empty");
            return Ok(Vec::new());
        }
    };

    // 引用方标题补全：锁定篇 title=None（UI 回退显示 note_id）
    let mut items = Vec::with_capacity(ids.len());
    for id in ids {
        let title = match core
            .kv_store
            .get(&format!("note:{id}"))
            .await
            .map_err(|e| e.to_string())?
        {
            Some(d) => match unwrap_note_bytes(&core, &vault, &d) {
                Ok(json) => json
                    .get("title")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string()),
                Err(_) => None,
            },
            None => None,
        };
        items.push(BacklinkItem { note_id: id, title });
    }
    Ok(items)
}
