//! DK-2.8 发布/分享（v1）：分享码全链——审阅报告 P2 功能。
//!
//! 数据链：cmd_create_share 读笔记（title+content+ts）→ JSON 打包 →
//! b64 分享码（前端复制/文件传递）；cmd_import_share 解码 → write_path
//! create_note 全纪律建新笔记（双写/blocks 派生/事件）+ rename。
//!
//! 语义边界（诚实化）：
//! - v1 分享码=文本传递（剪贴板/文件），**非 iroh 直传**——P2P 只读
//!   分享链接延后 v2（本机 endpoint 挂投影 doc+对端收件面，工程量大
//!   单列）；接收端拿到的是**副本**（改动不回传发布方——分享即断链）；
//! - 明文出库语义：分享码即明文（b64 非加密）——前端确认门（Export
//!   MarkdownGate 先例）+无水印溯源（v1 无追踪）。

use crate::{get_core, get_vault, unwrap_note_bytes};
use aurora_core::app_core::AppCore;
use aurora_security::LocalDekVault;
use serde::{Deserialize, Serialize};

/// 分享包（明文 JSON → b64 即分享码）。
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SharePack {
    pub title: String,
    pub content: String,
    pub created_at: String,
}

/// b64 编解码（快照域同款 STANDARD）。
fn b64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| format!("分享码解码失败: {e}"))
}

/// 生成分享码（笔记不存在/解密失败 fail-closed）。
///
/// # Errors
/// 应用未初始化 / 笔记不存在 / 解密失败 / KV 读失败。
pub async fn create_share_code(
    core: &AppCore,
    vault: &LocalDekVault,
    note_id: &str,
) -> Result<String, String> {
    let bytes = core
        .kv_store
        .get(&format!("note:{note_id}"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("笔记不存在: {note_id}"))?;
    let json = unwrap_note_bytes(core, vault, &bytes)?;
    let pack = SharePack {
        title: json
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("未命名")
            .to_string(),
        content: json
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string(),
        created_at: json
            .get("updated_at")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string(),
    };
    let plain = serde_json::to_vec(&pack).map_err(|e| e.to_string())?;
    Ok(b64_encode(&plain))
}

#[tauri::command]
pub async fn cmd_create_share(note_id: String) -> Result<String, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    create_share_code(&core, &vault, &note_id).await
}

#[tauri::command]
pub async fn cmd_import_share(code: String) -> Result<serde_json::Value, String> {
    let plain = b64_decode(&code)?;
    let pack: SharePack =
        serde_json::from_slice(&plain).map_err(|e| format!("分享包格式无效: {e}"))?;
    if pack.title.is_empty() && pack.content.is_empty() {
        return Err("分享包为空——拒绝导入".into());
    }
    let core = get_core()?;
    let vault = get_vault()?;
    let blocks = crate::blocks_store();
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
        core: core.clone(),
        blocks,
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
    let note_id = aurora_core::write_path::create_note(&ctx, &pack.title)
        .await
        .map_err(|e| e.to_string())?;
    if !pack.content.is_empty() {
        aurora_core::write_path::save_note_content(&ctx, &note_id, &pack.content)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(serde_json::json!({ "note_id": note_id, "title": pack.title }))
}

#[cfg(test)]
mod share_tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let pack = SharePack {
            title: "标题 <script>".into(), // 不可信内容原样承载——渲染侧防
            content: "# 正文\n- [ ] 待办".into(),
            created_at: "2026-10-18T00:00:00Z".into(),
        };
        let plain = serde_json::to_vec(&pack).unwrap();
        let code = b64_encode(&plain);
        let back: SharePack = serde_json::from_slice(&b64_decode(&code).unwrap()).unwrap();
        assert_eq!(back, pack);
    }

    #[test]
    fn bad_codes_rejected() {
        assert!(b64_decode("!!!非法!!!").is_err());
        let junk = b64_encode(b"not json");
        let parsed: Result<SharePack, _> = serde_json::from_slice(&b64_decode(&junk).unwrap());
        assert!(parsed.is_err());
    }
}
