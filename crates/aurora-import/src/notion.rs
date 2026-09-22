//! Notion 导出目录导入。
//!
//! Notion「Export → Markdown & CSV」产物：目录树内每个页面一个
//! `.md` 文件（文件名 = 页面标题 + 空格 + 32 位十六进制 hash 后缀），
//! 数据库页面另带同名 `.csv`。
//!
//! M3 策略：
//! - `.md`：复用 Markdown 通道（frontmatter/正文无损语义一致）；
//!   文件名剥离 hash 后缀作为标题兜底；
//! - `.csv`：数据库导出，逐库记 warning 跳过（结构化导入留后续切片）；
//! - 子页面目录：递归导入（walkdir 语义与 markdown 目录一致）。

use std::path::{Path, PathBuf};
use std::time::Instant;

use aurora_core::write_path::WriteContext;

use crate::report::{ImportError, ImportReport, ImportedEntry};

/// 从 Notion 导出文件名剥离标题（`页面标题 1234abcd…1234abcd.md` → `页面标题`）。
///
/// 规则：去掉扩展名后，若尾部存在连续 ≥ 32 个十六进制字符段，则将其
/// （及前导分隔空格）剥除。
pub fn notion_title_from_file_name(file_name: &str) -> String {
    let stem = file_name
        .trim_end_matches(".md")
        .trim_end_matches(".markdown");
    let trimmed = stem.trim_end();
    if trimmed.len() >= 32 {
        let bytes = trimmed.as_bytes();
        let mut end = trimmed.len();
        let mut count = 0usize;
        while end > 0 && count < 32 {
            let c = bytes[end - 1] as char;
            if c.is_ascii_hexdigit() {
                end -= 1;
                count += 1;
            } else {
                break;
            }
        }
        if count == 32 {
            let title = trimmed[..end].trim_end();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }
    stem.to_string()
}

/// 导入 Notion 导出目录。
///
/// # Errors
/// 目录不存在 / 不是目录。
pub async fn import_notion_export(
    ctx: &WriteContext,
    dir: &Path,
) -> Result<ImportReport, ImportError> {
    let started = Instant::now();
    if !dir.is_dir() {
        return Err(ImportError {
            path: dir.to_path_buf(),
            reason: "目录不存在或不是目录".to_string(),
        });
    }
    let mut report = ImportReport::default();

    // 先把 Notion 目录规整为「标题 → 内容」序列，再走统一的
    // markdown 语义（frontmatter/正文无损）。
    let walker = walkdir::WalkDir::new(dir).sort_by_file_name();
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                report.errors.push(ImportError {
                    path: dir.to_path_buf(),
                    reason: format!("遍历失败: {e}"),
                });
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path: PathBuf = entry.into_path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if name.ends_with(".csv") {
            report.skipped += 1;
            report
                .warnings
                .push(format!("{name}: Notion 数据库 CSV 导出暂不支持，已跳过"));
            continue;
        }
        let is_md = name.ends_with(".md") || name.ends_with(".markdown");
        if !is_md {
            report.skipped += 1;
            continue;
        }
        report.scanned += 1;

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                report.failed += 1;
                report.errors.push(ImportError {
                    path: path.clone(),
                    reason: format!("读取失败: {e}"),
                });
                continue;
            }
        };
        // 标题：frontmatter title 优先 → 文件名剥 hash → stem
        let (_, body, mut warnings) = crate::markdown::md_to_note(&content, "");
        let title = notion_title_from_file_name(&name);
        for w in warnings.drain(..) {
            report.warnings.push(format!("{name}: {w}"));
        }

        if ctx.attachments.is_some() {
            // 附件模式（§3.2）：与 markdown 同路径（notion 导出即 md + 资源目录）
            match aurora_core::write_path::create_note(ctx, &title).await {
                Ok(note_id) => {
                    let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
                    let mut cache = std::collections::HashMap::new();
                    let body = crate::attach::rewrite_md_images(
                        ctx,
                        &note_id,
                        &body,
                        &base,
                        &mut report,
                        &mut cache,
                    )
                    .await;
                    match aurora_core::write_path::save_note_content(ctx, &note_id, &body).await {
                        Ok(_) => {
                            report.imported += 1;
                            report.note_ids.push(note_id.clone());
                            report.entries.push(ImportedEntry {
                                note_id,
                                title,
                                source: path.display().to_string(),
                                tags: Vec::new(),
                                resources: Vec::new(),
                            });
                        }
                        Err(e) => {
                            let _ = aurora_core::write_path::delete_note(ctx, &note_id).await;
                            report.failed += 1;
                            report.errors.push(ImportError {
                                path: path.clone(),
                                reason: format!("写入失败: {e}"),
                            });
                        }
                    }
                }
                Err(e) => {
                    report.failed += 1;
                    report.errors.push(ImportError {
                        path: path.clone(),
                        reason: format!("写入失败: {e}"),
                    });
                }
            }
            continue;
        }
        match crate::import_one(ctx, &title, &body).await {
            Ok(note_id) => {
                report.imported += 1;
                report.note_ids.push(note_id.clone());
                report.entries.push(ImportedEntry {
                    note_id,
                    title,
                    source: path.display().to_string(),
                    tags: Vec::new(),
                    resources: Vec::new(),
                });
            }
            Err(e) => {
                report.failed += 1;
                report.errors.push(ImportError {
                    path: path.clone(),
                    reason: format!("写入失败: {e}"),
                });
            }
        }
    }

    report.duration_ms = started.elapsed().as_millis();
    tracing::info!(
        dir = %dir.display(),
        scanned = report.scanned,
        imported = report.imported,
        skipped = report.skipped,
        failed = report.failed,
        "notion export import done"
    );
    Ok(report)
}
