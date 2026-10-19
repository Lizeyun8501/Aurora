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
    import_share_code(&code).await
}

/// 导入链核心（DK-2.8v2 抽取：分享码/直传收件共用）——解码 → write_path
/// create_note 全纪律建新笔记 + 首段内容回写。
///
/// # Errors
/// 解码失败 / 分享包格式无效 / 空包 / write_path 建笔记失败。
pub(crate) async fn import_share_code(code: &str) -> Result<serde_json::Value, String> {
    let plain = b64_decode(code)?;
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

// ===========================================================================
// DK-2.8v2: iroh 直传分享 + 收件箱（延迟拉取模式——批复卡 2026-10-18）
//
// 链路：cmd_share_send 直传（门禁 DK-51 案二→配对 addr→send_share ack 语义）；
// 收件侧 run_share_accept_loop（setup 接线，lib.rs）落 `share:inbox:{ts}` KV；
// cmd_inbox_list 扫前缀列条目 / cmd_inbox_import 取件即建笔记+删条目。
//
// 语义边界（诚实化）：
// - 收件循环接线依赖 P2P 配对面（enable_p2p——DK-44 句柄已备、配对 UI 未落）
//   ——transport 就绪即生效（setup_share_accept 挂钩），配对面落地零改动；
// - 只读副本语义延续 v1：收到的即独立副本，改动不回传；
// - peer 标识 v1 缺省常量（配对面落地后带真实 EndpointId）。
// ===========================================================================

use crate::sync_commands::get_booted;
use aurora_sync::iroh_transport::PublicEndpointAddr;

/// 收件箱 KV 键前缀（条目键 = `share:inbox:{received_at_rfc3339}`）。
pub(crate) const INBOX_PREFIX: &str = "share:inbox:";

/// 收件箱条目（KV 序列化形态）。
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InboxEntry {
    /// SharePack JSON b64（与分享码同编码——import 链完全复用）
    pack_b64: String,
    peer: String,
    received_at: String,
}

/// 收件箱条目 DTO（前端列表行）。
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InboxItemDto {
    pub id: String,
    pub title: String,
    pub peer: String,
    pub received_at: String,
    pub content_len: usize,
}

/// iroh 直传：门禁 → 配对地址 → 打包 → send_share（等对端落箱 ack）。
///
/// # Errors
/// 应用未初始化 / 门禁推迟 / P2P 未启用 / 对端未配置 / 打包失败 / 直传失败。
#[tauri::command]
pub async fn cmd_share_send(note_id: String) -> Result<serde_json::Value, String> {
    let booted = get_booted()?;
    // DK-51 案二同款门禁：网络策略拒绝不进入直传（Defer 不是错误——
    // 错误信息携带 decision+reason，前端直接展示）。
    let decision = booted.sync_gate.evaluate();
    if !decision.is_allowed() {
        return Err(format!(
            "直传被门禁推迟（{decision:?}）：{}——检查网络或「仅 Wi-Fi」设置后重试",
            decision.reason()
        ));
    }
    let Some(transport) = booted.p2p() else {
        return Err("P2P 未启用：请先完成设备配对".into());
    };
    let addr_raw = booted
        .core
        .kv_store
        .get("sync.peer.addr")
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "尚未配置同步对端".to_string())?;
    let addr: PublicEndpointAddr =
        serde_json::from_slice(&addr_raw).map_err(|e| format!("对端地址解析失败: {e}"))?;
    let vault = get_vault()?;
    let code = create_share_code(&booted.core, &vault, &note_id).await?;
    // pack 字节 = 分享码解码（SharePack JSON 原文——传输面与 v1 码同载荷）
    let pack = b64_decode(&code)?;
    let report = transport.send_share(addr, &pack).await?;
    Ok(serde_json::json!({
        "sent": report.sent_bytes,
        "peer": report.remote_peer,
    }))
}

/// 列收件箱（scan_prefix 前缀扫——收件顺序倒序展示）。
///
/// # Errors
/// 应用未初始化 / KV 扫描失败。
#[tauri::command]
pub async fn cmd_inbox_list() -> Result<Vec<InboxItemDto>, String> {
    let booted = get_booted()?;
    let items = booted
        .core
        .kv_store
        .scan_prefix(INBOX_PREFIX)
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (key, value) in items {
        // 损坏条目跳过不阻断列表（诚实降级——不静默删，用户可清空重建）
        let Ok(entry) = serde_json::from_slice::<InboxEntry>(&value) else {
            continue;
        };
        let (title, content_len) = match b64_decode(&entry.pack_b64) {
            Ok(plain) => match serde_json::from_slice::<SharePack>(&plain) {
                Ok(p) => {
                    let len = p.content.chars().count();
                    (
                        if p.title.is_empty() {
                            "未命名".into()
                        } else {
                            p.title
                        },
                        len,
                    )
                }
                Err(_) => ("(格式损坏条目)".into(), 0),
            },
            Err(_) => ("(编码损坏条目)".into(), 0),
        };
        out.push(InboxItemDto {
            id: key,
            title,
            peer: entry.peer,
            received_at: entry.received_at,
            content_len,
        });
    }
    out.sort_by(|a, b| b.received_at.cmp(&a.received_at));
    Ok(out)
}

/// 取件导入：读条目 → import_share_code 全纪律建笔记 → 删条目（取件即清）。
///
/// # Errors
/// 应用未初始化 / 条目不存在 / 建笔记失败 / KV 删除失败。
#[tauri::command]
pub async fn cmd_inbox_import(id: String) -> Result<serde_json::Value, String> {
    let booted = get_booted()?;
    let raw = booted
        .core
        .kv_store
        .get(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "收件条目不存在".to_string())?;
    let entry: InboxEntry =
        serde_json::from_slice(&raw).map_err(|e| format!("收件条目损坏: {e}"))?;
    let result = import_share_code(&entry.pack_b64).await?;
    booted
        .core
        .kv_store
        .delete(&id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result)
}

/// setup 接线：P2P transport 就绪时 spawn 分享收件循环（收件落
/// `share:inbox:{received_at}` KV——async 写经 tauri runtime spawn）。
///
/// transport 未就绪（配对面未接线）时跳过并 info——配对面落地后
/// 重新调用本函数即可生效（v1 无需热加载：重启应用即重试）。
pub(crate) fn setup_share_accept(booted: std::sync::Arc<aurora_bootstrap::BootedApp>) {
    let Some(transport) = booted.p2p() else {
        tracing::info!("share accept loop skipped: P2P transport not enabled yet");
        return;
    };
    let kv = booted.core.kv_store.clone();
    let sink: aurora_sync::iroh_transport::ShareInboxSink =
        std::sync::Arc::new(move |pack: Vec<u8>, peer: String| {
            let kv = kv.clone();
            tauri::async_runtime::spawn(async move {
                let received_at = chrono::Utc::now().to_rfc3339();
                let key = format!("{INBOX_PREFIX}{received_at}");
                let pack_b64 = {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.encode(&pack)
                };
                let entry = InboxEntry {
                    pack_b64,
                    peer,
                    received_at,
                };
                match serde_json::to_vec(&entry) {
                    Ok(bytes) => {
                        if let Err(e) = kv.set(&key, &bytes).await {
                            tracing::error!("share inbox write failed: {}", e);
                        }
                    }
                    Err(e) => tracing::error!("share inbox encode failed: {}", e),
                }
            });
        });
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    // _shutdown_tx 常驻（app 生命周期——v1 不做运行时关闭；drop 即关闭语义
    // 由 watch 保留端保证，循环只看 false）
    std::mem::forget(_shutdown_tx);
    tauri::async_runtime::spawn(aurora_sync::iroh_transport::run_share_accept_loop(
        transport,
        shutdown_rx,
        sink,
    ));
    tracing::info!("share accept loop started");
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
