//! DK-09 渲染接线 command 层（attachment:// scheme 解析 · Alpha 切片
//! 2026-09-23）。
//!
//! # 设计
//! - 读取经 `write_path::read_attachment` 唯一入口（解封 + sha256 完整性
//!   校验 fail-closed），不旁路 store 裸取。
//! - id 白名单校验（`validate_attachment_id`）前置：id 参与 KV 键拼接，
//!   非法 id 一律拒绝（键穿越 / 分隔符注入防护，与 core 单测对齐）。
//! - DTO 序列化 base64（IPC JSON 通道），大小上限 20 MiB——大 blob 导出
//!   另行切片，防 IPC 通道阻塞。
//! - ctx 装配复用 `import_commands::import_ctx`（同源语义：APP_STATE /
//!   VAULT_STATE / ATTACH_STATE，seal 对 = vault DEK at-rest）。

use base64::Engine as _;
use serde::Serialize;

use aurora_core::write_path::read_attachment;

/// `cmd_read_attachment` 返回 DTO（serde 直通，前端 `<img src=data:...>` /
/// 文件卡直接消费）。
#[derive(Serialize)]
pub struct ReadAttachmentDto {
    pub attachment_id: String,
    pub note_id: String,
    pub file_name: String,
    pub mime: String,
    pub size: u64,
    /// 明文字节（seal 模式已解封 + 完整性校验通过）。
    pub data_base64: String,
}

/// IPC 单包上限：附件读取走 JSON base64，超限拒绝（导出切片另行处理）。
const MAX_ATTACHMENT_BYTES: usize = 20 * 1024 * 1024;

/// 读取附件（attachment:// scheme 解析后端）。
///
/// # Errors
/// - id 非白名单（注入防护，core `validate_attachment_id`）
/// - 应用未初始化 / 附件缺失 / 解封或完整性校验失败（fail-closed 透传）
/// - 超过 `MAX_ATTACHMENT_BYTES`
#[tauri::command]
pub async fn cmd_read_attachment(attachment_id: String) -> Result<ReadAttachmentDto, String> {
    if !aurora_core::attachment_store::validate_attachment_id(&attachment_id) {
        return Err("非法附件 id".into());
    }
    let ctx = super::import_commands::import_ctx()?;
    let (meta, plaintext) = read_attachment(&ctx, &attachment_id)
        .await
        .map_err(|e| format!("附件读取失败: {e}"))?;
    if plaintext.len() > MAX_ATTACHMENT_BYTES {
        return Err(format!("附件超过 IPC 上限（{} > 20 MiB）", plaintext.len()));
    }
    Ok(ReadAttachmentDto {
        attachment_id: meta.attachment_id,
        note_id: meta.note_id,
        file_name: meta.file_name,
        mime: meta.mime,
        size: meta.size,
        data_base64: base64::engine::general_purpose::STANDARD.encode(&plaintext),
    })
}
