//! DK-09 第一切片：Markdown 目录导入（M2）。
//!
//! 用户把一个 Markdown 目录（含子目录）交给 Aurora，目录里每个 `.md`
//! 文件变成一篇笔记，块结构无损。写入严格走 aurora-core `write_path`
//! 唯一入口（`create_note` + `save_note_content`），不旁路落盘。
//!
//! # 内容无损底线（任务书 §1）
//!
//! - 正文（body）逐字符原样写入，代码块/表格/任务列表/行尾风格均保留；
//! - 变换仅限：frontmatter 块剥离 + 标题确定（见 [`markdown`]）；
//! - frontmatter 解析失败 → 原样导入 + 警告，绝不丢内容。
//!
//! # 用法
//!
//! ```no_run
//! use aurora_import::{import_markdown_dir, ImportOptions};
//!
//! # async fn example(ctx: &aurora_core::write_path::WriteContext) -> Result<(), Box<dyn std::error::Error>> {
//! let report = import_markdown_dir(ctx, std::path::Path::new("~/notes"), &ImportOptions::default()).await?;
//! println!("{report}");
//! # Ok(())
//! # }
//! ```

pub mod markdown;
pub mod report;

use std::path::Path;
use std::time::Instant;

use aurora_core::write_path::WriteContext;
pub use report::ImportError;
use report::ImportReport;

/// 导入选项。
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// 是否导入隐藏文件（`. 开头文件/目录内）。默认跳过（防 `.obsidian` 等误入）。
    pub include_hidden: bool,
    /// 最大递归深度（walkdir 语义：根目录 = 0，根下文件 = 1）。
    /// `usize::MAX` = 不限。
    pub max_depth: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            max_depth: usize::MAX,
        }
    }
}

/// 扫描 `dir`（递归）并将其中每个 `.md` / `.markdown` 文件导入为笔记。
///
/// 流程（每文件）：读文本 → frontmatter 剥离/标题确定（[`markdown::md_to_note`]）
/// → `write_path::create_note`（标题）→ `write_path::save_note_content`（正文）。
/// 单文件失败记入报告并继续（`failed` + `errors`）。
///
/// 遍历按文件名排序（`walkdir::sort_by_file_name`），保证同输入同顺序。
pub async fn import_markdown_dir(
    ctx: &WriteContext,
    dir: &Path,
    options: &ImportOptions,
) -> Result<ImportReport, ImportError> {
    if !dir.is_dir() {
        return Err(ImportError {
            path: dir.to_path_buf(),
            reason: "目录不存在或不是目录".to_string(),
        });
    }
    let started = Instant::now();
    let mut report = ImportReport::default();

    let walker = walkdir::WalkDir::new(dir)
        .max_depth(options.max_depth)
        .sort_by_file_name();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                report.failed += 1;
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
        let path = entry.into_path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // 隐藏判定：相对根的任一路径分量以 `.` 开头（覆盖隐藏目录整棵跳过）
        let relative = path.strip_prefix(dir).unwrap_or(&path);
        let is_hidden = relative
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
        if is_hidden && !options.include_hidden {
            report.skipped += 1;
            continue;
        }

        let is_markdown = name.ends_with(".md") || name.ends_with(".markdown");
        if !is_markdown {
            report.skipped += 1;
            continue;
        }
        report.scanned += 1;

        // 读文本 → 变换
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
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.clone());
        let (title, body, warnings) = markdown::md_to_note(&content, &stem);
        for w in warnings {
            report.warnings.push(format!("{name}: {w}"));
        }

        // write_path 唯一入口写入
        match import_one(ctx, &title, &body).await {
            Ok(note_id) => {
                report.imported += 1;
                report.note_ids.push(note_id);
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
        "markdown dir import done"
    );
    Ok(report)
}

/// 单文件写入：`create_note`（标题）+ `save_note_content`（正文）。
async fn import_one(
    ctx: &WriteContext,
    title: &str,
    body: &str,
) -> Result<String, aurora_core::Error> {
    let note_id = aurora_core::write_path::create_note(ctx, title).await?;
    aurora_core::write_path::save_note_content(ctx, &note_id, body).await?;
    Ok(note_id)
}
