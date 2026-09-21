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

pub mod enex;
pub mod enml;
pub mod html;
pub mod markdown;
pub mod notion;
pub mod opml;
pub mod report;
pub mod wizard;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use aurora_core::write_path::WriteContext;
pub use notion::import_notion_export;
pub use opml::import_opml_file;
pub use report::ImportError;
use report::{ImportReport, ImportedEntry, ResourceInfo};
pub use wizard::{
    content_hash_hex, plan_enex_file, plan_markdown_dir, plan_opml_file, ImportKind,
    ImportManifest, ImportPlan, ManifestEntry, PlanItem, ProgressEvent,
};

/// 导入选项。
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// 是否导入隐藏文件（`. 开头文件/目录内）。默认跳过（防 `.obsidian` 等误入）。
    pub include_hidden: bool,
    /// 最大递归深度（walkdir 语义：根目录 = 0，根下文件 = 1）。
    /// `usize::MAX` = 不限。
    pub max_depth: usize,
    /// 会话清单目录（防重）。`Some(dir)` 时读 `dir/manifest.json`，
    /// 同 (source, content_hash) 跳过，成功后追加写回。`None` = 不防重。
    pub manifest_dir: Option<PathBuf>,
    /// 进度推送通道（每处理一个条目发一条，含跳过）。
    pub progress: Option<tokio::sync::mpsc::UnboundedSender<ProgressEvent>>,
    /// 选择性导入白名单（source 相对路径；空 = 全量）。支持目录前缀。
    pub only: Vec<String>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            max_depth: usize::MAX,
            manifest_dir: None,
            progress: None,
            only: Vec::new(),
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

    let mut manifest = options
        .manifest_dir
        .as_deref()
        .map(wizard::ImportManifest::load)
        .unwrap_or_default();

    // 第一遍：收集待处理清单（隐藏/非 md/only 过滤；错误记账）
    let mut files: Vec<(PathBuf, String)> = Vec::new();
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
        // 选择性导入：only 白名单（相对路径精确或目录前缀）
        let rel = relative.to_string_lossy().replace('\\', "/");
        if !options.only.is_empty()
            && !options
                .only
                .iter()
                .any(|o| o == &rel || rel.starts_with(&format!("{o}/")))
        {
            continue;
        }
        files.push((path, rel));
    }
    report.scanned = files.len();
    let total = files.len() as u32;

    // 第二遍：写库（manifest 防重 + 进度推送）
    for (i, (path, rel)) in files.into_iter().enumerate() {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // 读文本 → 变换
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                report.failed += 1;
                report.errors.push(ImportError {
                    path: path.clone(),
                    reason: format!("读取失败: {e}"),
                });
                if let Some(tx) = &options.progress {
                    let _ = tx.send(ProgressEvent {
                        current: i as u32 + 1,
                        total,
                        source: rel.clone(),
                    });
                }
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

        // 会话清单防重：同 (source, hash) 跳过
        let digest = wizard::content_hash_hex(content.as_bytes());
        if manifest.contains(&rel, &digest) {
            report.skipped += 1;
            if let Some(tx) = &options.progress {
                let _ = tx.send(ProgressEvent {
                    current: i as u32 + 1,
                    total,
                    source: rel.clone(),
                });
            }
            continue;
        }

        // write_path 唯一入口写入
        match import_one(ctx, &title, &body).await {
            Ok(note_id) => {
                report.imported += 1;
                report.note_ids.push(note_id.clone());
                manifest.record(wizard::ManifestEntry {
                    source: rel.clone(),
                    content_hash: digest,
                    note_id: note_id.clone(),
                    title: title.clone(),
                });
                report.entries.push(ImportedEntry {
                    note_id,
                    title,
                    source: rel.clone(),
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
        if let Some(tx) = &options.progress {
            let _ = tx.send(ProgressEvent {
                current: i as u32 + 1,
                total,
                source: rel,
            });
        }
    }

    if let Some(mdir) = &options.manifest_dir {
        manifest.save(mdir).map_err(|e| ImportError {
            path: mdir.join("manifest.json"),
            reason: format!("清单写盘失败: {e}"),
        })?;
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

/// ENEX 导入选项。
#[derive(Debug, Clone, Default)]
pub struct EnexImportOptions {
    /// 会话清单目录（防重，键 `<file>#<标题>` + 内容指纹）。
    pub manifest_dir: Option<PathBuf>,
    /// 进度推送通道（每 note 一条，含跳过）。
    pub progress: Option<tokio::sync::mpsc::UnboundedSender<ProgressEvent>>,
    /// 选择性导入白名单（source = `<file>#<标题>`；空 = 全量）。
    pub only: Vec<String>,
    /// 资源 sidecar 落盘目录。`None` = 不落盘：en-media 输出
    /// `attachment://<hash>` 占位 + warning（等待 core 附件 API，见
    /// `issues/wf/bravo-request-attachment-store-api.md`）。
    pub attachments_dir: Option<PathBuf>,
}

/// base64 解码（容忍 ENEX 数据中的空白/换行）。
fn decode_base64(s: &str) -> Result<Vec<u8>, String> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    B64.decode(cleaned.as_bytes())
        .map_err(|e| format!("base64: {e}"))
}

/// HTML 导入选项（M3：目前无附加项，占位保持 API 形状稳定）。
#[derive(Debug, Clone, Default)]
pub struct HtmlImportOptions {}

/// 导入单个 HTML 文件（要求良构，见 [`html`] 模块说明）。
///
/// 标题规则：`<title>` 元素 → 首个 `h1` → 文件名 stem。
///
/// # Errors
/// 文件读取失败 / HTML 非良构。
pub async fn import_html_file(
    ctx: &WriteContext,
    html_path: &Path,
    _options: &HtmlImportOptions,
) -> Result<ImportReport, ImportError> {
    let started = Instant::now();
    let raw = std::fs::read_to_string(html_path).map_err(|e| ImportError {
        path: html_path.to_path_buf(),
        reason: format!("读取失败: {e}"),
    })?;
    let conv = html::html_to_markdown(&raw).map_err(|e| ImportError {
        path: html_path.to_path_buf(),
        reason: format!("HTML 转换失败: {e}"),
    })?;

    let mut report = ImportReport {
        scanned: 1,
        ..Default::default()
    };

    let title = html::extract_title_element(&raw)
        .or_else(|| html::extract_first_heading(&conv.markdown))
        .unwrap_or_else(|| {
            html_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "未命名 HTML".to_string())
        });

    match import_one(ctx, &title, &conv.markdown).await {
        Ok(note_id) => {
            report.imported += 1;
            report.note_ids.push(note_id.clone());
            report.entries.push(ImportedEntry {
                note_id,
                title,
                source: html_path.display().to_string(),
                tags: Vec::new(),
                resources: Vec::new(),
            });
        }
        Err(e) => {
            report.failed += 1;
            report.errors.push(ImportError {
                path: html_path.to_path_buf(),
                reason: format!("写入失败: {e}"),
            });
        }
    }
    report.warnings.extend(conv.warnings);
    report.duration_ms = started.elapsed().as_millis();
    tracing::info!(file = %html_path.display(), "html import done");
    Ok(report)
}

/// sidecar 落盘文件名：`<hash 前 12 位>-<basename>`（防撞名 + 防目录逃逸）。
fn safe_attachment_name(file_name: Option<&str>, mime: &str, hash: &str) -> String {
    let base = match file_name {
        Some(n) => Path::new(n)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| n.to_string()),
        None => {
            let ext = mime.split('/').next_back().unwrap_or("bin");
            format!("{hash}.{ext}")
        }
    };
    format!("{}-{}", &hash[..hash.len().min(12)], base)
}

/// mime → 占位文件名的回退扩展名。
fn placeholder_name(mime: &str, hash: &str) -> String {
    let ext = mime.split('/').next_back().unwrap_or("bin");
    format!("{hash}.{ext}")
}

/// 导入 ENEX 文件（印象笔记导出，Evernote Export DTD 3）。
///
/// 流程：quick-xml 流式解析（[`enex::parse_enex`]）→ 逐 note ENML→Markdown
/// 转换（[`enml::enml_to_markdown`]）→ `write_path` 唯一入口写入。
///
/// 资源处理：[`EnexImportOptions::attachments_dir`] 提供时 base64 解码落
/// sidecar 文件（`<hash12>-<basename>`），en-media 占位指向该文件；
/// 未提供时输出 `attachment://<hash>` 占位 + warning（附件 API 待
/// core 侧落地，见 request 文档）。
///
/// 单 note 失败记入报告并继续。
pub async fn import_enex(
    ctx: &WriteContext,
    enex_path: &Path,
    options: &EnexImportOptions,
) -> Result<ImportReport, ImportError> {
    let started = Instant::now();
    let xml = std::fs::read_to_string(enex_path).map_err(|e| ImportError {
        path: enex_path.to_path_buf(),
        reason: format!("读取失败: {e}"),
    })?;
    let notes = enex::parse_enex(&xml).map_err(|e| ImportError {
        path: enex_path.to_path_buf(),
        reason: format!("ENEX 解析失败: {e}"),
    })?;

    let mut report = ImportReport {
        scanned: notes.len(),
        ..Default::default()
    };
    // 稳定键（only/manifest/进度）：`<文件名>#<idx>`——路径移动不破坏防重；
    // entries.source 仍用全路径（展示）
    let source_base = enex_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| enex_path.display().to_string());
    let entry_source_base = enex_path.display().to_string();

    // 资源 sidecar 落盘（同 hash 全局只落一次）
    let mut resource_infos: HashMap<String, ResourceInfo> = HashMap::new();
    if let Some(att_dir) = &options.attachments_dir {
        std::fs::create_dir_all(att_dir).map_err(|e| ImportError {
            path: att_dir.clone(),
            reason: format!("附件目录创建失败: {e}"),
        })?;
        for note in &notes {
            for r in &note.resources {
                if resource_infos.contains_key(&r.hash) {
                    continue;
                }
                let bytes = match decode_base64(&r.data_base64) {
                    Ok(b) => b,
                    Err(e) => {
                        report
                            .warnings
                            .push(format!("资源 {} 解码失败，已跳过落盘: {e}", &r.hash));
                        continue;
                    }
                };
                let safe = safe_attachment_name(r.file_name.as_deref(), &r.mime, &r.hash);
                let out_path = att_dir.join(&safe);
                if let Err(e) = std::fs::write(&out_path, &bytes) {
                    report
                        .warnings
                        .push(format!("资源 {} 落盘失败: {e}", &r.hash));
                    continue;
                }
                resource_infos.insert(
                    r.hash.clone(),
                    ResourceInfo {
                        hash: r.hash.clone(),
                        mime: r.mime.clone(),
                        file_name: r.file_name.clone(),
                        bytes: bytes.len(),
                        written_to: Some(out_path),
                    },
                );
            }
        }
    }

    let mut manifest = options
        .manifest_dir
        .as_deref()
        .map(wizard::ImportManifest::load)
        .unwrap_or_default();

    // 选择性导入：only 白名单预过滤（source = `<file>#<idx>`）
    let selected: Vec<(usize, &enex::RawEnexNote)> = notes
        .iter()
        .enumerate()
        .filter(|(idx, _)| {
            options.only.is_empty()
                || options
                    .only
                    .iter()
                    .any(|o| o == &format!("{source_base}#{idx}"))
        })
        .collect();
    let total = selected.len() as u32;

    for (k, (idx, note)) in selected.into_iter().enumerate() {
        let title = if note.title.trim().is_empty() {
            format!("未命名笔记 {}", idx + 1)
        } else {
            note.title.clone()
        };
        let source = format!("{source_base}#{idx}");
        let entry_source = format!("{entry_source_base}#{idx}");

        // en-media 占位映射：hash → (alt 文本, 链接目标)。
        // 有 sidecar：alt=原始文件名，link=attachments/<落盘名>；
        // 无 sidecar：alt=占位名，link=attachment://<hash>（附件 API 待落地）。
        let mut res_map: HashMap<String, (String, String)> = HashMap::new();
        let mut res_infos: Vec<ResourceInfo> = Vec::new();
        for r in &note.resources {
            let info = resource_infos.get(&r.hash);
            let (alt, link) = match info {
                Some(i) => {
                    let alt = i
                        .file_name
                        .clone()
                        .unwrap_or_else(|| placeholder_name(&i.mime, &r.hash));
                    let link = i
                        .written_to
                        .as_ref()
                        .map(|p| {
                            let name = p
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| placeholder_name(&i.mime, &r.hash));
                            format!("attachments/{name}")
                        })
                        .unwrap_or_else(|| format!("attachment://{}", &r.hash));
                    (alt, link)
                }
                None => {
                    report.warnings.push(format!(
                        "[{title}] 资源 {} 未落盘（未提供 attachments_dir），使用 attachment:// 占位",
                        &r.hash
                    ));
                    let ph = placeholder_name(&r.mime, &r.hash);
                    (ph.clone(), format!("attachment://{}", &r.hash))
                }
            };
            res_map.insert(r.hash.clone(), (alt, link));
            if let Some(i) = info {
                res_infos.push(i.clone());
            }
        }

        let conv = match enml::enml_to_markdown(&note.content, &res_map) {
            Ok(c) => c,
            Err(e) => {
                report.failed += 1;
                report.errors.push(ImportError {
                    path: PathBuf::from(&entry_source),
                    reason: format!("ENML 转换失败: {e}"),
                });
                if let Some(tx) = &options.progress {
                    let _ = tx.send(ProgressEvent {
                        current: k as u32 + 1,
                        total,
                        source: source.clone(),
                    });
                }
                continue;
            }
        };
        for w in conv.warnings {
            report.warnings.push(format!("[{title}] {w}"));
        }

        // 会话清单防重：内容指纹 = 标题 + 正文
        let digest = wizard::content_hash_hex(format!("{title}\u{1f}{}", conv.markdown).as_bytes());
        if manifest.contains(&source, &digest) {
            report.skipped += 1;
            if let Some(tx) = &options.progress {
                let _ = tx.send(ProgressEvent {
                    current: k as u32 + 1,
                    total,
                    source: source.clone(),
                });
            }
            continue;
        }

        match import_one(ctx, &title, &conv.markdown).await {
            Ok(note_id) => {
                report.imported += 1;
                report.note_ids.push(note_id.clone());
                manifest.record(wizard::ManifestEntry {
                    source: source.clone(),
                    content_hash: digest,
                    note_id: note_id.clone(),
                    title: title.clone(),
                });
                report.entries.push(ImportedEntry {
                    note_id,
                    title,
                    source: entry_source,
                    tags: note.tags.clone(),
                    resources: res_infos,
                });
            }
            Err(e) => {
                report.failed += 1;
                report.errors.push(ImportError {
                    path: PathBuf::from(&entry_source),
                    reason: format!("写入失败: {e}"),
                });
            }
        }
        if let Some(tx) = &options.progress {
            let _ = tx.send(ProgressEvent {
                current: k as u32 + 1,
                total,
                source: source.clone(),
            });
        }
    }

    if let Some(mdir) = &options.manifest_dir {
        manifest.save(mdir).map_err(|e| ImportError {
            path: mdir.join("manifest.json"),
            reason: format!("清单写盘失败: {e}"),
        })?;
    }

    report.duration_ms = started.elapsed().as_millis();
    tracing::info!(
        file = %enex_path.display(),
        scanned = report.scanned,
        imported = report.imported,
        failed = report.failed,
        warnings = report.warnings.len(),
        "enex import done"
    );
    Ok(report)
}
