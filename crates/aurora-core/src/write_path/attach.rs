//! B-04: 附件域（write_path 拆分——纯搬移）。
use super::*;

// ===== 附件写入入口（DK-09 · request 裁决落地）=====

/// 附件写入（唯一入口）——数据经 `WriteContext.seal` 字节封装（桌面 vault
/// DEK at-rest；移动 None 明文降级），内容寻址去重（sha256 blob 键）。
///
/// 不触碰笔记内容版本（`updated_at`/blocks/事件流均不动——附件不是正文变更）。
/// `ctx.attachments` 为 None 时 fail-closed（调用方未注入附件能力）。
pub async fn attach_to_note(
    ctx: &WriteContext,
    note_id: &str,
    file_name: &str,
    mime: &str,
    data: &[u8],
) -> Result<crate::attachment_store::AttachmentMeta, Error> {
    let store = ctx
        .attachments
        .as_ref()
        .ok_or_else(|| Error::Crypto("no attachment store configured".into()))?;

    // 笔记存在性（附件必须挂在真实笔记上）
    load_note_meta(ctx.core.as_ref(), note_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;

    // 字节封装（明文降级 = 移动端语义）
    let stored: Vec<u8> = match ctx.seal.as_ref() {
        Some(pair) => (pair.seal)(data)?,
        None => data.to_vec(),
    };

    let meta = crate::attachment_store::make_meta(note_id, file_name, mime, data);
    // existed（blob 已复用）当前编排层不消费——去重计数由导入器经
    // get_blob 预检统计（bravo-request-put-existed-flag 批复口径）。
    let _existed = store.put(&meta, &stored).await?;
    info!(note_id, attachment_id = %meta.attachment_id, size = meta.size, "attachment stored via WritePath");
    Ok(meta)
}

/// 附件读取（唯一入口）——取密封字节 → 解封 → 明文 sha256 完整性校验
/// （fail-closed：任何不匹配都不返回数据）。
pub async fn read_attachment(
    ctx: &WriteContext,
    attachment_id: &str,
) -> Result<(crate::attachment_store::AttachmentMeta, Vec<u8>), Error> {
    let store = ctx
        .attachments
        .as_ref()
        .ok_or_else(|| Error::Crypto("no attachment store configured".into()))?;

    let meta = store
        .get_meta(attachment_id)
        .await?
        .ok_or(Error::NoteNotFound {
            id: attachment_id.to_string(),
        })?;
    let sealed = store
        .get_blob(&meta.sha256)
        .await?
        .ok_or_else(|| Error::Crypto(format!("attachment blob missing: {attachment_id}")))?;

    let plaintext: Vec<u8> = match ctx.seal.as_ref() {
        Some(pair) => (pair.unseal)(&sealed)?,
        None => sealed,
    };

    let actual = crate::attachment_store::sha256_hex(&plaintext);
    if actual != meta.sha256 {
        return Err(Error::Crypto(format!(
            "attachment '{attachment_id}' integrity check failed (expected {}, got {actual})",
            meta.sha256
        )));
    }
    Ok((meta, plaintext))
}
