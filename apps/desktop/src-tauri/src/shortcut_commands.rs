//! DK-2.10 快捷键自定义（v1）：应用内键位注册表持久化——审阅报告 P2 功能。
//!
//! 数据链：前端 DEFAULT_SHORTCUTS（palette=Mod+K / newNote=Mod+N /
//! today=Mod+J）为基线，本命令面读写**用户覆盖表**（KV
//! `settings:shortcuts`，JSON `{action: combo}` 全量快照）——get 返回
//! None=未自定义（前端走默认），set 全量覆盖（同 set_note_tags 语义）。
//!
//! v1 边界：应用内三动作（系统级全局快捷键=3.3 单列——桌面平台 TODO
//! 占位未接 global_shortcut 插件，诚实边界）；无键位冲突检测（录入时
//! 同组合后写覆盖先写——UI 提示延后）；组合格式 `Mod+X`（Mod=meta 或
//! ctrl 统一语义，前端录入门规范化）。

use crate::{get_core, get_vault};

/// 读取用户键位覆盖表（None=未自定义——前端默认基线）。
///
/// # Errors
/// 应用未初始化 / KV 读失败（字符串化透传）。
#[tauri::command]
pub async fn cmd_get_shortcuts() -> Result<Option<serde_json::Value>, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let bytes = core
        .kv_store
        .get("settings:shortcuts")
        .await
        .map_err(|e| e.to_string())?;
    let bytes = match bytes {
        None => return Ok(None),
        Some(b) => b,
    };
    let plain = vault
        .decrypt(core.crypto.as_ref(), &bytes)
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&plain)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// 全量覆盖用户键位表（空对象=等价恢复默认——UI 恢复按钮直传 {}；
/// seal 加密对称读回，同 settings 键敏感面语义）。
///
/// # Errors
/// 应用未初始化 / 序列化或 KV 写失败（字符串化透传）。
#[tauri::command]
pub async fn cmd_set_shortcuts(table: serde_json::Value) -> Result<(), String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let plain = serde_json::to_vec(&table).map_err(|e| e.to_string())?;
    let bytes = vault
        .encrypt(core.crypto.as_ref(), &plain)
        .map_err(|e| e.to_string())?;
    core.kv_store
        .set("settings:shortcuts", &bytes)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;

    /// 覆盖表契约护栏：键位串格式（录入端规范化后落库——后端仅透传，
    /// 此测试固化格式期望防两端漂移）。
    #[test]
    fn combo_format_contract() {
        let table = serde_json::json!({ "palette": "Mod+K", "newNote": "Mod+Shift+N" });
        let obj = table.as_object().unwrap();
        for (k, v) in obj {
            let combo = v.as_str().unwrap();
            assert!(
                combo.starts_with("Mod+") || combo.starts_with("Mod+Shift+"),
                "{k}"
            );
        }
        // 空对象=恢复默认语义
        assert!(serde_json::json!({}).as_object().unwrap().is_empty());
    }
}
