//! OPML（大纲交换格式）导入 — 幕布 / Workflowy / Dynalist 等导出。
//!
//! OPML 结构：`<opml><head><title>…</title></head><body><outline
//! text="节点" _note="正文"><outline …/></outline>…</body></opml>`。
//!
//! 映射策略：**每个顶层 `<outline>` 子树 → 一篇笔记**——标题取
//! `text`，正文 = 该子树递归转 Markdown 嵌套列表（`_note` 属性作为
//! 节点正文行，输出在列表项下方缩进块）。
//!
//! `_note` 中的换行按 `  \n`（MD 硬换行）保留。

use std::fmt::Write as _;
use std::time::Instant;

use aurora_core::write_path::WriteContext;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::report::{ImportError, ImportReport, ImportedEntry};

/// OPML 大纲节点（解析后）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutlineNode {
    /// `text` 属性（节点标题行）。
    pub text: String,
    /// `_note` 属性（节点正文，可缺失）。
    pub note: Option<String>,
    /// 子节点。
    pub children: Vec<OutlineNode>,
}

/// 解析 OPML 文本为顶层大纲节点列表。
///
/// # Errors
/// XML 损坏 / 缺 `<body>`。
pub fn parse_opml(xml: &str) -> Result<Vec<OutlineNode>, String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut roots: Vec<OutlineNode> = Vec::new();

    // 定位 <body>
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"body" => break,
            Ok(Event::Eof) => return Err("OPML 缺少 <body>".to_string()),
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        buf.clear();
    }

    // body 下每个顶层 outline 一棵子树
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"outline" => {
                let node = parse_outline(&mut reader, e)?;
                roots.push(node);
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == b"body" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        buf.clear();
    }
    Ok(roots)
}

/// 递归解析一个 `<outline>`（光标已消费 Start）。
fn parse_outline(
    reader: &mut Reader<&[u8]>,
    start: quick_xml::events::BytesStart<'_>,
) -> Result<OutlineNode, String> {
    let mut node = OutlineNode::default();
    for attr in start.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"text" => {
                node.text = attr
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map_err(|e| e.to_string())?
                    .into_owned();
            }
            b"_note" => {
                node.note = Some(
                    attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|e| e.to_string())?
                        .into_owned(),
                );
            }
            _ => {}
        }
    }

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"outline" => {
                let child = parse_outline(reader, e)?;
                node.children.push(child);
            }
            // 自闭合 outline（无子节点）会以 Empty 事件出现
            Ok(Event::Empty(e)) if e.local_name().as_ref() == b"outline" => {
                let mut child = OutlineNode::default();
                for attr in e.attributes().flatten() {
                    match attr.key.local_name().as_ref() {
                        b"text" => {
                            child.text = attr
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map_err(|err| err.to_string())?
                                .into_owned();
                        }
                        b"_note" => {
                            child.note = Some(
                                attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|err| err.to_string())?
                                    .into_owned(),
                            );
                        }
                        _ => {}
                    }
                }
                node.children.push(child);
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == b"outline" => return Ok(node),
            Ok(Event::Eof) => return Err("outline 未闭合（遇到 EOF）".to_string()),
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        buf.clear();
    }
}

/// 大纲子树 → Markdown 嵌套列表（不含根自身标题行）。
fn outline_to_markdown(node: &OutlineNode, depth: usize, out: &mut String) {
    for child in &node.children {
        let indent = "  ".repeat(depth);
        let _ = writeln!(out, "{indent}- {}", child.text);
        if let Some(note) = &child.note {
            // _note 多行 → 缩进块保留硬换行
            for line in note.lines() {
                let _ = writeln!(out, "{indent}    {line}");
            }
        }
        outline_to_markdown(child, depth + 1, out);
    }
}

/// 导入 OPML 文件：每个顶层 outline → 一篇笔记。
///
/// # Errors
/// 文件读取失败 / OPML 损坏。
pub async fn import_opml_file(
    ctx: &WriteContext,
    opml_path: &std::path::Path,
) -> Result<ImportReport, ImportError> {
    let started = Instant::now();
    let xml = std::fs::read_to_string(opml_path).map_err(|e| ImportError {
        path: opml_path.to_path_buf(),
        reason: format!("读取失败: {e}"),
    })?;
    let roots = parse_opml(&xml).map_err(|e| ImportError {
        path: opml_path.to_path_buf(),
        reason: format!("OPML 解析失败: {e}"),
    })?;

    let mut report = ImportReport {
        scanned: roots.len(),
        ..Default::default()
    };
    let source_base = opml_path.display().to_string();

    for (idx, root) in roots.iter().enumerate() {
        let title = if root.text.trim().is_empty() {
            format!("未命名大纲 {}", idx + 1)
        } else {
            root.text.clone()
        };
        let source = format!("{source_base}#{idx}");

        let mut body = String::new();
        if let Some(note) = &root.note {
            body.push_str(note);
            body.push('\n');
        }
        outline_to_markdown(root, 0, &mut body);

        match crate::import_one(ctx, &title, &body).await {
            Ok(note_id) => {
                report.imported += 1;
                report.note_ids.push(note_id.clone());
                report.entries.push(ImportedEntry {
                    note_id,
                    title,
                    source,
                    tags: Vec::new(),
                    resources: Vec::new(),
                });
            }
            Err(e) => {
                report.failed += 1;
                report.errors.push(ImportError {
                    path: std::path::PathBuf::from(&source),
                    reason: format!("写入失败: {e}"),
                });
            }
        }
    }

    report.duration_ms = started.elapsed().as_millis();
    tracing::info!(
        file = %opml_path.display(),
        scanned = report.scanned,
        imported = report.imported,
        failed = report.failed,
        "opml import done"
    );
    Ok(report)
}
