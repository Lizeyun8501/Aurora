//! DK-2.6 版本历史（v1）：编辑器内容历史链——审阅报告 P2 功能。
//!
//! 数据链：cmd_update_note content 写成功后追加密文历史快照
//! （`notehist:{user_id}:{note_id}:{ts_ms}` → seal(content)）→ 列表/读取/
//! 回滚走本命令面。**loro OpLog 全历史仍在 doc 内部**（底座版本化），
//! 本 v1 是 desktop 域编辑态粗粒度快照链（20 版上限/同内容去抖），
//! loro checkout 精细回滚延后 v2（跨 crate 签名预批流程）。
//!
//! v1 边界：无 per-版本 label/备注；无 diff 高亮（UI 行级简易对比）；
//! 历史非关键路径——追加失败静默 warn 不阻断保存主链。

use crate::{get_core, get_vault};
use aurora_core::app_core::AppCore;
use aurora_security::LocalDekVault;
use serde::Serialize;

/// 每笔记保留版本上限（超出删最旧）。
const HISTORY_CAP: usize = 20;

fn hist_key(user_id: &str, note_id: &str, ts_ms: u64) -> String {
    format!("notehist:{user_id}:{note_id}:{ts_ms}")
}

fn hist_prefix(user_id: &str, note_id: &str) -> String {
    format!("notehist:{user_id}:{note_id}:")
}

/// 追加历史版本（同内容去抖：最新一版明文比对一致则跳过）。
/// 非关键路径调用方处理错误（update_note 内 fire-and-forget）。
pub async fn push_note_history(
    core: &AppCore,
    vault: &LocalDekVault,
    note_id: &str,
    content: &str,
) -> Result<(), String> {
    // 去抖：最新一版内容一致 → 跳过
    let prefix = hist_prefix(&core.user_id, note_id);
    let rows = core
        .kv_store
        .scan_prefix(&prefix)
        .await
        .map_err(|e| e.to_string())?;
    if let Some((_, latest)) = rows.iter().max_by_key(|(k, _)| k.clone()) {
        if let Ok(plain) = vault.decrypt(core.crypto.as_ref(), latest) {
            if plain == content.as_bytes() {
                return Ok(());
            }
        }
    }

    let sealed = vault
        .encrypt(core.crypto.as_ref(), content.as_bytes())
        .map_err(|e| e.to_string())?;
    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    core.kv_store
        .set(&hist_key(&core.user_id, note_id, ts_ms), &sealed)
        .await
        .map_err(|e| e.to_string())?;

    // 超上限删最旧（含刚写入的当前版——按 ts 升序保留最新 CAP 版）
    if rows.len() + 1 > HISTORY_CAP {
        let mut keys: Vec<String> = rows.iter().map(|(k, _)| k.clone()).collect();
        keys.sort();
        let overflow = (rows.len() + 1) - HISTORY_CAP;
        for k in keys.into_iter().take(overflow) {
            let _ = core.kv_store.delete(&k).await;
        }
    }
    Ok(())
}

/// 历史版本元数据（ts 升序——UI 倒序呈现）。
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// 版本时间戳（毫秒）。
    pub ts: u64,
    /// 密文体积（字节）——仅展示用。
    pub size: usize,
}

/// 列出笔记历史版本（trash/Folder 不特判——历史仅随 update_note 产生，
/// 无写路径即无历史，天然空集）。
///
/// # Errors
/// 应用未初始化 / KV 读失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_list_note_histories(note_id: String) -> Result<Vec<HistoryEntry>, String> {
    let core = get_core()?;
    let prefix = hist_prefix(&core.user_id, &note_id);
    let rows = core
        .kv_store
        .scan_prefix(&prefix)
        .await
        .map_err(|e| e.to_string())?;
    let mut out: Vec<HistoryEntry> = rows
        .iter()
        .filter_map(|(k, v)| {
            let ts: u64 = k.rsplit(':').next()?.parse().ok()?;
            Some(HistoryEntry { ts, size: v.len() })
        })
        .collect();
    out.sort_by_key(|e| e.ts);
    Ok(out)
}

/// 读取单版本明文。
///
/// # Errors
/// 版本不存在 / 解密失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_get_note_history(note_id: String, ts: u64) -> Result<String, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let bytes = core
        .kv_store
        .get(&hist_key(&core.user_id, &note_id, ts))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("历史版本不存在: {ts}"))?;
    let plain = vault
        .decrypt(core.crypto.as_ref(), &bytes)
        .map_err(|e| format!("历史版本解密失败: {e}"))?;
    String::from_utf8(plain).map_err(|e| format!("历史版本非 UTF-8: {e}"))
}

#[cfg(test)]
mod history_tests {
    use super::*;

    #[test]
    fn key_shapes() {
        let k = hist_key("u1", "n1", 12345);
        assert!(k.starts_with("notehist:u1:n1:"));
        assert_eq!(hist_prefix("u1", "n1"), "notehist:u1:n1:");
        // 键尾 ts 可逆解析（列表路径依赖）
        let ts: u64 = k.rsplit(':').next().unwrap().parse().unwrap();
        assert_eq!(ts, 12345);
    }

    #[test]
    fn cap_constant() {
        // 上限语义护栏：清理逻辑依赖 CAP 与 scan 行数关系
        assert!(HISTORY_CAP > 0 && HISTORY_CAP <= 100);
    }
}
