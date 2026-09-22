//! 附件接管公共层（DK-09 收官切片 · 任务书 bravo-DK09-import-attachments）。
//!
//! 引用形态（Alpha 定调 §2，无需 request）：图片 `![file-name](attachment://{id})`，
//! 其余资源 `[file-name](attachment://{id})`。附件写入只经
//! `write_path::attach_to_note` 唯一入口，禁止旁路 `store.put`。

use crate::report::ImportReport;
use aurora_core::write_path::{attach_to_note, WriteContext};
use std::path::Path;

/// 扩展名 → MIME（导出场景常见类型；未知回落 application/octet-stream）。
pub(crate) fn mime_from_ext(path: &Path) -> String {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("txt") | Some("md") => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// 扫描正文中的 Markdown 图片语法 `![alt](target)`，对相对路径资源
/// （存在且可读）执行 attach 并重写为 `attachment://{id}`；缺失/不可读/
/// attach 失败 → 计入 `attachments_missing`，链接原样保留（宽松语义，
/// 导入不失败）。绝对 URL（http/https）、`attachment://`、`data:`、
/// 纯锚点跳过。同笔记内同一路径复用同一 attachment id（`cache` 由调用
/// 方按笔记持有）。无附件能力（`ctx.attachments` None）时原样返回。
pub(crate) async fn rewrite_md_images(
    ctx: &WriteContext,
    note_id: &str,
    body: &str,
    base_dir: &Path,
    report: &mut ImportReport,
    cache: &mut std::collections::HashMap<String, String>,
) -> String {
    if ctx.attachments.is_none() || !body.contains("![") {
        return body.to_string();
    }
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < bytes.len() {
        // 找 image 语法的 `![`
        if !body[i..].starts_with("![") {
            out.push(body[i..].chars().next().unwrap_or('\0'));
            i += body[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            continue;
        }
        // alt 至 `](`；target 至 `)`（目标不含空白即终止——与 CommonMark
        // 无尖括号路径一致；导出产物路径均无空白形态，空白路径保留原样）
        let open_alt = i + 1; // `!` 之后 `[` 下标
        let Some(close_alt) = body[open_alt..].find(']').map(|p| open_alt + p) else {
            out.push_str(&body[i..i + 2]);
            i += 2;
            continue;
        };
        if !body[close_alt + 1..].starts_with('(') {
            out.push_str(&body[i..=close_alt]);
            i = close_alt + 1;
            continue;
        }
        let open_target = close_alt + 1;
        let Some(close_target) = body[open_target..].find(')').map(|p| open_target + p) else {
            out.push_str(&body[i..=close_alt]);
            i = close_alt + 1;
            continue;
        };
        let target = &body[open_target + 1..close_target];
        let skip = target.contains("://")
            || target.starts_with("attachment://")
            || target.starts_with("data:")
            || target.starts_with('#')
            || target.trim().is_empty();
        if skip {
            out.push_str(&body[i..=close_target]);
            i = close_target + 1;
            continue;
        }
        // 相对路径解析（拒绝 `..` 越界源目录）
        let rel = target.trim();
        let resolved = base_dir.join(rel);
        let inside = resolved.starts_with(base_dir);
        let data = if inside {
            tokio::fs::read(&resolved).await.ok()
        } else {
            None
        };
        match data {
            Some(data) => {
                let file_name = resolved
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "resource".to_string());
                let mime = mime_from_ext(&resolved);
                // 同笔记内同路径复用 id（cache 命中则不再 attach）
                if let Some(id) = cache.get(resolved.to_string_lossy().as_ref()) {
                    let id = id.clone();
                    out.push_str(&body[i..open_target + 1]);
                    out.push_str(&format!("attachment://{id}"));
                    out.push(')');
                    i = close_target + 1;
                    continue;
                }
                let id = match attach_to_note(ctx, note_id, &file_name, &mime, &data).await {
                    Ok(meta) => meta.attachment_id,
                    Err(e) => {
                        report.attachments_missing += 1;
                        tracing::warn!(note_id, file = %rel, "markdown 资源 attach 失败（宽松降级）: {e}");
                        out.push_str(&body[i..=close_target]);
                        i = close_target + 1;
                        continue;
                    }
                };
                report.attachments_imported += 1;
                cache.insert(resolved.to_string_lossy().into_owned(), id.clone());
                out.push_str(&body[i..open_target + 1]);
                out.push_str(&format!("attachment://{id}"));
                out.push(')');
                i = close_target + 1;
            }
            None => {
                report.attachments_missing += 1;
                out.push_str(&body[i..=close_target]);
                i = close_target + 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::mime_from_ext;
    use std::path::Path;

    #[test]
    fn dk09_mime_from_ext() {
        assert_eq!(mime_from_ext(Path::new("a.png")), "image/png");
        assert_eq!(mime_from_ext(Path::new("b.JPEG")), "image/jpeg");
        assert_eq!(
            mime_from_ext(Path::new("c.unknown")),
            "application/octet-stream"
        );
        assert_eq!(
            mime_from_ext(Path::new("noext")),
            "application/octet-stream"
        );
    }
}
