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
    /// 明文字节（seal 模式已解封 + 完整性校验通过;range 模式下为切片）。
    pub data_base64: String,
    /// A-02 范围读取元信息：None=全量;Some((offset, len))=明文切片区间。
    /// 解密/完整性校验恒全量（AES-GCM 整块语义——分片仅作用于 IPC 传输层）。
    pub range: Option<(u64, u64)>,
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
pub async fn cmd_read_attachment(
    attachment_id: String,
    /// A-02 范围读取（Option 向后兼容: 旧调用不传=全量）。
    /// `{start, len}`——明文区间切片;解密/校验恒全量（fail-closed 不旁路）。
    range: Option<AttachmentRange>,
) -> Result<ReadAttachmentDto, String> {
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
    // A-02 范围读取：大文件不整块进 IPC（文本类预览按需取片）。
    // 越界 clamp 到明文尾（宽容语义——前端按 range 元信息判断 EOF）。
    let total = plaintext.len() as u64;
    let (slice, range_meta) = match range {
        Some(r) => {
            let start = r.start.min(total);
            let end = (start + r.len).min(total);
            let slice = &plaintext[start as usize..end as usize];
            (slice, Some((start, slice.len() as u64)))
        }
        None => (plaintext.as_slice(), None),
    };
    Ok(ReadAttachmentDto {
        attachment_id: meta.attachment_id,
        note_id: meta.note_id,
        file_name: meta.file_name,
        mime: meta.mime,
        size: meta.size,
        data_base64: base64::engine::general_purpose::STANDARD.encode(slice),
        range: range_meta,
    })
}

/// A-02 范围读取参数（camelCase IPC 透传——serde rename 双端一致）。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRange {
    pub start: u64,
    pub len: u64,
}

// ===== A-02（TA 14:31 工单）: 附件落盘导出 + 系统应用原子打开 =====

/// 附件另存（unseal→完整性校验→落盘 dest_dir;sanitize/dedupe 惯例）。
/// 「用系统应用打开」的数据底座——Office/PDF 类预览降级的出口通路。
///
/// # Errors
/// 非法 id / 附件缺失 / 解封或校验失败（fail-closed）/ 目录创建或写盘失败。
#[tauri::command]
pub async fn cmd_export_attachment(
    attachment_id: String,
    dest_dir: String,
) -> Result<String, String> {
    if !aurora_core::attachment_store::validate_attachment_id(&attachment_id) {
        return Err("非法附件 id".into());
    }
    let ctx = super::import_commands::import_ctx()?;
    let (meta, plaintext) = read_attachment(&ctx, &attachment_id)
        .await
        .map_err(map_read_err)?;
    if plaintext.len() > MAX_ATTACHMENT_BYTES {
        return Err(format!("附件超过 IPC 上限（{} > 20 MiB）", plaintext.len()));
    }

    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("create dir `{dest_dir}`: {e}"))?;
    let mut existing: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut rd = tokio::fs::read_dir(&dest_dir)
        .await
        .map_err(|e| format!("read dir: {e}"))?;
    while let Some(e) = rd.next_entry().await.map_err(|e| format!("readdir: {e}"))? {
        if let Some(name) = e.file_name().to_str() {
            existing.insert(name.to_string());
        }
    }
    let base = aurora_core::export::sanitize_filename(&meta.file_name);
    // 任意扩展名（附件原名保真）——dedupe 按全名基去重。
    let existing_any: std::collections::HashSet<String> = existing
        .into_iter()
        .map(|n| n.trim_end_matches(".bin").to_string())
        .collect();
    let mut final_name = base.clone();
    let mut n = 1usize;
    while existing_any.contains(&format!("{final_name}.bin")) || existing_any.contains(&final_name)
    {
        final_name = format!("{base}-{n}");
        n += 1;
    }
    let path = format!("{}/{}", dest_dir.trim_end_matches('/'), final_name);
    tokio::fs::write(&path, &plaintext)
        .await
        .map_err(|e| format!("write `{path}`: {e}"))?;
    Ok(path)
}

/// 系统应用原子打开（unseal→临时目录落盘→Rust 侧 shell open——不经前端
/// capability 面，插件已在 Builder 注册）。临时文件名=附件原名（dedupe 单调）。
///
/// # Errors
/// 同 cmd_export_attachment + shell open 失败。
#[tauri::command]
pub async fn cmd_open_attachment(
    app: tauri::AppHandle,
    attachment_id: String,
) -> Result<String, String> {
    if !aurora_core::attachment_store::validate_attachment_id(&attachment_id) {
        return Err("非法附件 id".into());
    }
    let ctx = super::import_commands::import_ctx()?;
    let (meta, plaintext) = read_attachment(&ctx, &attachment_id)
        .await
        .map_err(map_read_err)?;
    if plaintext.len() > MAX_ATTACHMENT_BYTES {
        return Err(format!("附件超过 IPC 上限（{} > 20 MiB）", plaintext.len()));
    }
    // 系统临时目录（std::env::temp_dir）——预览缓存语义:每次打开新建
    //（临时目录由 OS 生命周期管理,不属应用缓存——锁定态零内容渲染纪律
    // 不受影响:unseal 失败在此前已 fail-closed 拒绝）。
    let dir = std::env::temp_dir().join("aurora-attachment-preview");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("create temp dir: {e}"))?;
    let base = aurora_core::export::sanitize_filename(&meta.file_name);
    let path = dir.join(&base);
    tokio::fs::write(&path, &plaintext)
        .await
        .map_err(|e| format!("write temp: {e}"))?;
    let path_str = path.to_string_lossy().into_owned();

    // Rust 侧 shell open（ShellExt——generate_handler 自动注入 AppHandle,
    // 不经前端 capability 面）。
    use tauri_plugin_shell::ShellExt as _;
    app.shell()
        .open(&path_str, None)
        .map_err(|e| format!("shell open: {e}"))?;
    Ok(path_str)
}

/// read_attachment 错误映射（IPC 层文案）。
fn map_read_err(e: aurora_core::Error) -> String {
    e.to_string()
}
