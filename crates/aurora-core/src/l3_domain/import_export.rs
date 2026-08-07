//! 导入导出系统（Import / Export System）
//!
//! 实现 Markdown 导入、Notion 迁移、PDF 导出、批量导入（Zip + manifest + 断点续传）。
//!
//! # 简化说明
//! - pulldown-cmark 不在工作区依赖中，因此 SubTask 3.4.1 的 Markdown 解析使用
//!   手写的简易逐行解析器，覆盖标题、段落、代码块、列表、加粗/斜体等基础语法。
//! - ProseMirror doc tree 用 `serde_json::Value` 表示中间结构。
//! - PDF 导出仅生成可打印的 HTML 字符串，不调用真实 headless Chrome。

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

use super::content_editor::{Block, BlockType, Document};

// ============================================================================
// SubTask 3.4.1: Markdown 导入
// ============================================================================

/// 导入格式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    /// Markdown 文本
    Markdown,
    /// Notion 导出包
    Notion,
    /// HTML 文本
    Html,
}

/// 导出格式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Markdown,
    Pdf,
    Html,
    Json,
}

/// YAML frontmatter 解析结果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Frontmatter {
    pub fields: HashMap<String, serde_json::Value>,
}

impl Frontmatter {
    /// 从 Markdown 文本头部解析 `---` 包裹的 frontmatter 块。
    /// 简化实现：仅支持 `key: value` 形式，value 不做类型推断（统一存为字符串）。
    pub fn parse(markdown: &str) -> (Self, String) {
        let mut fm = Self::default();
        let mut body = markdown.to_string();

        if markdown.starts_with("---\n") || markdown.starts_with("---\r\n") {
            if let Some(end) = markdown[4..].find("\n---") {
                let fm_block = &markdown[4..4 + end];
                for line in fm_block.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        let key = k.trim().to_string();
                        let val = v.trim().trim_matches('"').to_string();
                        fm.fields.insert(key, serde_json::Value::String(val));
                    }
                }
                // 跳过结束的 `---` 行
                let body_start = 4 + end + 4; // `\n---` 长度 4
                body = markdown[body_start..]
                    .trim_start_matches(['\n', '\r'])
                    .to_string();
            }
        }

        (fm, body)
    }

    /// 序列化为 YAML 字符串（简化版）
    pub fn to_yaml(&self) -> String {
        if self.fields.is_empty() {
            return String::new();
        }
        let mut out = String::from("---\n");
        for (k, v) in &self.fields {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push_str(&format!("{}: \"{}\"\n", k, s));
        }
        out.push_str("---\n");
        out
    }
}

/// 简易 Markdown 中间 AST 节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MdNode {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    Code { language: String, code: String },
    ListItem { ordered: bool, text: String },
    TodoItem { checked: bool, text: String },
    Quote { text: String },
    Divider,
}

/// 简易 Markdown 解析器（逐行）
///
/// # 支持语法
/// - `# ~ ######` 标题
/// - ``` 代码块（带语言）
/// - `- ` / `* ` 无序列表
/// - `1. ` 有序列表
/// - `- [ ]` / `- [x]` 待办项
/// - `> ` 引用
/// - `---` 分割线
/// - 普通段落
/// - 行内 `**bold**` / `*italic*` 保留原文（不展开为 marks）
pub struct MarkdownParser;

impl MarkdownParser {
    pub fn parse(text: &str) -> Vec<MdNode> {
        let mut nodes = Vec::new();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // 代码块
            if line.trim_start().starts_with("```") {
                let lang = line.trim_start()[3..].trim().to_string();
                let mut code = String::new();
                i += 1;
                while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                    if !code.is_empty() {
                        code.push('\n');
                    }
                    code.push_str(lines[i]);
                    i += 1;
                }
                // 跳过结束 ```
                if i < lines.len() {
                    i += 1;
                }
                nodes.push(MdNode::Code {
                    language: lang,
                    code,
                });
                continue;
            }

            // 空行
            if line.trim().is_empty() {
                i += 1;
                continue;
            }

            // 标题
            if let Some(stripped) = line.strip_prefix('#') {
                let mut level = 1u8;
                let mut rest = stripped;
                while rest.starts_with('#') && level < 6 {
                    level += 1;
                    rest = &rest[1..];
                }
                if let Some(stripped) = rest.strip_prefix(' ') {
                    nodes.push(MdNode::Heading {
                        level,
                        text: stripped.trim().to_string(),
                    });
                    i += 1;
                    continue;
                }
            }

            // 分割线
            if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
                nodes.push(MdNode::Divider);
                i += 1;
                continue;
            }

            // 待办项
            let trimmed = line.trim_start();
            if trimmed.starts_with("- [ ] ")
                || trimmed.starts_with("- [x] ")
                || trimmed.starts_with("- [X] ")
            {
                let checked =
                    trimmed.chars().nth(3) == Some('x') || trimmed.chars().nth(3) == Some('X');
                let text = trimmed[6..].to_string();
                nodes.push(MdNode::TodoItem { checked, text });
                i += 1;
                continue;
            }

            // 无序列表
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                nodes.push(MdNode::ListItem {
                    ordered: false,
                    text: trimmed[2..].to_string(),
                });
                i += 1;
                continue;
            }

            // 有序列表
            if let Some(idx) = trimmed.find(". ") {
                let prefix = &trimmed[..idx];
                if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
                    nodes.push(MdNode::ListItem {
                        ordered: true,
                        text: trimmed[idx + 2..].to_string(),
                    });
                    i += 1;
                    continue;
                }
            }

            // 引用
            if let Some(stripped) = trimmed.strip_prefix("> ") {
                let mut quote_lines = vec![stripped.to_string()];
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim_start();
                    if let Some(stripped) = l.strip_prefix("> ") {
                        quote_lines.push(stripped.to_string());
                        i += 1;
                    } else {
                        break;
                    }
                }
                nodes.push(MdNode::Quote {
                    text: quote_lines.join("\n"),
                });
                continue;
            }

            // 段落（连续非空行合并）
            let mut para = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if l.trim().is_empty() {
                    break;
                }
                // 遇到特殊语法行则停止
                let lt = l.trim_start();
                if lt.starts_with('#')
                    || lt.starts_with("```")
                    || lt.starts_with("- ")
                    || lt.starts_with("* ")
                    || lt.starts_with("> ")
                    || lt == "---"
                    || (lt.starts_with(|c: char| c.is_ascii_digit()) && lt.contains(". "))
                {
                    break;
                }
                para.push(l.to_string());
                i += 1;
            }
            if !para.is_empty() {
                nodes.push(MdNode::Paragraph {
                    text: para.join("\n"),
                });
            }
        }

        nodes
    }
}

/// ProseMirror 风格的文档树（用 JSON 表示）
pub fn to_prosemirror_doc(nodes: &[MdNode], title: &str) -> serde_json::Value {
    let content: Vec<serde_json::Value> = nodes
        .iter()
        .map(|n| match n {
            MdNode::Heading { level, text } => serde_json::json!({
                "type": "heading",
                "attrs": {"level": level},
                "content": [{"type": "text", "text": text}]
            }),
            MdNode::Paragraph { text } => serde_json::json!({
                "type": "paragraph",
                "content": [{"type": "text", "text": text}]
            }),
            MdNode::Code { language, code } => serde_json::json!({
                "type": "code_block",
                "attrs": {"language": language},
                "content": [{"type": "text", "text": code}]
            }),
            MdNode::ListItem { ordered, text } => serde_json::json!({
                "type": "list_item",
                "attrs": {"ordered": ordered},
                "content": [{"type": "text", "text": text}]
            }),
            MdNode::TodoItem { checked, text } => serde_json::json!({
                "type": "todo_item",
                "attrs": {"checked": checked},
                "content": [{"type": "text", "text": text}]
            }),
            MdNode::Quote { text } => serde_json::json!({
                "type": "blockquote",
                "content": [{"type": "text", "text": text}]
            }),
            MdNode::Divider => serde_json::json!({"type": "horizontal_rule"}),
        })
        .collect();

    serde_json::json!({
        "type": "doc",
        "attrs": {"title": title},
        "content": content
    })
}

/// 将 Markdown AST 转换为现有 `Document`/`Block` 类型
pub fn nodes_to_document(nodes: &[MdNode], title: &str, frontmatter: &Frontmatter) -> Document {
    let mut doc = Document::new(title);
    for n in nodes {
        let block = match n {
            MdNode::Heading { level, text } => Block::heading(*level, text.clone()),
            MdNode::Paragraph { text } => Block::text(text.clone()),
            MdNode::Code { language, code } => Block::code(language.clone(), code.clone()),
            MdNode::ListItem { text, .. } => Block::list_item(text.clone()),
            MdNode::TodoItem { checked, text } => Block::todo(*checked, text.clone()),
            MdNode::Quote { text } => Block::quote(text.clone()),
            MdNode::Divider => Block::divider(),
        };
        doc = doc.with_block(block);
    }
    // frontmatter 写入 properties
    for (k, v) in &frontmatter.fields {
        doc.properties.insert(k.clone(), v.clone());
    }
    doc
}

// ============================================================================
// SubTask 3.4.2: Notion 迁移
// ============================================================================

/// Notion 块类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NotionBlockType {
    Paragraph,
    Heading1,
    Heading2,
    Heading3,
    BulletedListItem,
    NumberedListItem,
    ToDo,
    Code,
    Quote,
    Divider,
    Image,
    Callout,
    Embed,
    ChildDatabase,
    Unknown(String),
}

/// Notion 富文本注解（对应 Notion API 的 annotations 对象）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotionAnnotations {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub underline: bool,
    pub code: bool,
    pub color: Option<String>,
}

/// Notion 富文本片段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionRichText {
    pub plain_text: String,
    pub href: Option<String>,
    pub annotations: NotionAnnotations,
}

/// Notion 块（来自 Notion API 导出）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionBlock {
    pub id: String,
    pub block_type: NotionBlockType,
    pub rich_text: Vec<NotionRichText>,
    pub children: Vec<NotionBlock>,
    pub properties: HashMap<String, serde_json::Value>,
}

impl NotionBlock {
    /// 将富文本片段拼接为纯文本
    pub fn plain_text(&self) -> String {
        self.rich_text
            .iter()
            .map(|r| r.plain_text.clone())
            .collect()
    }

    /// 将 Notion 块类型映射为内部 `BlockType`
    pub fn to_block_type(&self) -> BlockType {
        match self.block_type {
            NotionBlockType::Paragraph => BlockType::Text,
            NotionBlockType::Heading1 => BlockType::Heading,
            NotionBlockType::Heading2 => BlockType::Heading,
            NotionBlockType::Heading3 => BlockType::Heading,
            NotionBlockType::BulletedListItem => BlockType::ListItem,
            NotionBlockType::NumberedListItem => BlockType::ListItem,
            NotionBlockType::ToDo => BlockType::TodoItem,
            NotionBlockType::Code => BlockType::Code,
            NotionBlockType::Quote => BlockType::Quote,
            NotionBlockType::Divider => BlockType::Divider,
            NotionBlockType::Image => BlockType::Image,
            NotionBlockType::Callout => BlockType::Quote,
            NotionBlockType::Embed => BlockType::Custom("embed".to_string()),
            NotionBlockType::ChildDatabase => BlockType::Custom("collection".to_string()),
            NotionBlockType::Unknown(ref s) => BlockType::Custom(s.clone()),
        }
    }

    /// 转换为内部 `Block`，并应用富文本注解为 marks
    pub fn to_block(&self) -> Block {
        let block_type = self.to_block_type();
        let mut block = match self.block_type {
            NotionBlockType::Heading1 => Block::heading(1, self.plain_text()),
            NotionBlockType::Heading2 => Block::heading(2, self.plain_text()),
            NotionBlockType::Heading3 => Block::heading(3, self.plain_text()),
            NotionBlockType::ToDo => {
                let checked = self
                    .properties
                    .get("checked")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Block::todo(checked, self.plain_text())
            }
            NotionBlockType::Code => {
                let lang = self
                    .properties
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("plaintext")
                    .to_string();
                Block::code(lang, self.plain_text())
            }
            NotionBlockType::Image => {
                let url = self
                    .properties
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Block::image(url, self.plain_text())
            }
            _ => Block::new(block_type, self.plain_text()),
        };

        // 收集 marks（富文本注解）
        let marks: Vec<serde_json::Value> = self
            .rich_text
            .iter()
            .filter(|r| {
                r.annotations.bold
                    || r.annotations.italic
                    || r.annotations.strikethrough
                    || r.annotations.underline
                    || r.annotations.code
            })
            .map(|r| {
                let mut marks = Vec::new();
                if r.annotations.bold {
                    marks.push(serde_json::json!({"type": "bold"}));
                }
                if r.annotations.italic {
                    marks.push(serde_json::json!({"type": "italic"}));
                }
                if r.annotations.strikethrough {
                    marks.push(serde_json::json!({"type": "strike"}));
                }
                if r.annotations.underline {
                    marks.push(serde_json::json!({"type": "underline"}));
                }
                if r.annotations.code {
                    marks.push(serde_json::json!({"type": "code"}));
                }
                serde_json::json!({"text": r.plain_text, "marks": marks})
            })
            .collect();
        if !marks.is_empty() {
            block
                .properties
                .insert("marks".to_string(), serde_json::json!(marks));
        }
        block
    }
}

/// Notion 数据库 → Collection 概念（简化为带 schema 的表格块）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionCollection {
    pub id: String,
    pub title: String,
    pub schema: HashMap<String, serde_json::Value>,
    pub rows: Vec<HashMap<String, serde_json::Value>>,
}

/// Notion 迁移器
pub struct NotionMigration;

impl NotionMigration {
    /// 将一批 Notion 块迁移为内部 `Document`
    pub fn migrate(title: &str, blocks: &[NotionBlock]) -> Document {
        let mut doc = Document::new(title);
        for b in blocks {
            doc = doc.with_block(b.to_block());
        }
        doc
    }

    /// 将 Notion child_database 转换为 Collection
    pub fn to_collection(block: &NotionBlock) -> NotionCollection {
        NotionCollection {
            id: block.id.clone(),
            title: block
                .properties
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string(),
            schema: block
                .properties
                .get("schema")
                .and_then(|v| v.as_object())
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
            rows: Vec::new(),
        }
    }
}

// ============================================================================
// SubTask 3.4.3: PDF 导出
// ============================================================================

/// PDF 模板（生成可打印 HTML 的模板）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfTemplate {
    pub name: String,
    pub css: String,
    pub header_html: Option<String>,
    pub footer_html: Option<String>,
    pub page_size: PageSize,
    pub margin_mm: f32,
}

impl Default for PdfTemplate {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            css: DEFAULT_PDF_CSS.to_string(),
            header_html: None,
            footer_html: None,
            page_size: PageSize::A4,
            margin_mm: 20.0,
        }
    }
}

/// 页面尺寸
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PageSize {
    A4,
    Letter,
    Legal,
}

impl PageSize {
    pub fn mm(&self) -> (f32, f32) {
        match self {
            PageSize::A4 => (210.0, 297.0),
            PageSize::Letter => (215.9, 279.4),
            PageSize::Legal => (215.9, 355.6),
        }
    }
}

const DEFAULT_PDF_CSS: &str = r#"
body { font-family: -apple-system, "Helvetica Neue", Arial, sans-serif; line-height: 1.6; color: #222; }
h1 { font-size: 24pt; border-bottom: 2px solid #333; padding-bottom: 4pt; }
h2 { font-size: 18pt; }
h3 { font-size: 14pt; }
pre { background: #f5f5f5; padding: 8pt; border-radius: 4pt; font-family: "JetBrains Mono", monospace; }
code { font-family: "JetBrains Mono", monospace; }
blockquote { border-left: 4px solid #ccc; margin: 0; padding-left: 12pt; color: #555; }
table { border-collapse: collapse; }
th, td { border: 1px solid #ccc; padding: 4pt 8pt; }
@page { margin: 20mm; }
"#;

/// PDF 导出配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfExportConfig {
    pub template: PdfTemplate,
    pub include_toc: bool,
    pub include_page_numbers: bool,
    pub batch: bool,
}

impl Default for PdfExportConfig {
    fn default() -> Self {
        Self {
            template: PdfTemplate::default(),
            include_toc: false,
            include_page_numbers: true,
            batch: false,
        }
    }
}

/// PDF 导出器（生成可打印 HTML）
pub struct PdfExporter {
    config: PdfExportConfig,
}

impl PdfExporter {
    pub fn new(config: PdfExportConfig) -> Self {
        Self { config }
    }

    /// 将单个文档渲染为可打印 HTML
    pub fn render_html(&self, doc: &Document) -> String {
        let mut html = String::new();
        html.push_str("<!DOCTYPE html><html><head><meta charset=\"utf-8\">");
        html.push_str(&format!("<title>{}</title>", escape_html(&doc.title)));
        html.push_str("<style>");
        html.push_str(&self.config.template.css);
        html.push_str("</style></head><body>");

        if let Some(header) = &self.config.template.header_html {
            html.push_str(header);
        }

        html.push_str(&format!("<h1>{}</h1>", escape_html(&doc.title)));

        for block in &doc.blocks {
            html.push_str(&render_block_html(block));
        }

        if self.config.include_page_numbers {
            html.push_str(
                r#"<div class="page-footer">Page <span class="page-number"></span> / <span class="page-count"></span></div>"#,
            );
        }

        if let Some(footer) = &self.config.template.footer_html {
            html.push_str(footer);
        }

        html.push_str("</body></html>");
        html
    }

    /// 批量渲染多个文档为合并 HTML（用分页符分隔）
    pub fn render_batch(&self, docs: &[Document]) -> String {
        let mut html = String::new();
        html.push_str("<!DOCTYPE html><html><head><meta charset=\"utf-8\">");
        html.push_str("<style>");
        html.push_str(&self.config.template.css);
        html.push_str("</style></head><body>");
        for (i, doc) in docs.iter().enumerate() {
            if i > 0 {
                html.push_str("<div style=\"page-break-after: always;\"></div>");
            }
            html.push_str(&format!("<h1>{}</h1>", escape_html(&doc.title)));
            for block in &doc.blocks {
                html.push_str(&render_block_html(block));
            }
        }
        html.push_str("</body></html>");
        html
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_block_html(block: &Block) -> String {
    match block.block_type {
        BlockType::Text => format!(
            "<p>{}</p>",
            escape_html(block.content.as_str().unwrap_or(""))
        ),
        BlockType::Heading => {
            let level = block
                .properties
                .get("level")
                .and_then(|v| v.as_u64())
                .unwrap_or(1)
                .clamp(1, 6);
            format!(
                "<h{lvl}>{txt}</h{lvl}>",
                lvl = level,
                txt = escape_html(block.content.as_str().unwrap_or(""))
            )
        }
        BlockType::Code => {
            let lang = block
                .properties
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!(
                "<pre><code class=\"language-{}\">{}</code></pre>",
                lang,
                escape_html(block.content.as_str().unwrap_or(""))
            )
        }
        BlockType::Quote => format!(
            "<blockquote>{}</blockquote>",
            escape_html(block.content.as_str().unwrap_or(""))
        ),
        BlockType::ListItem => format!(
            "<li>{}</li>",
            escape_html(block.content.as_str().unwrap_or(""))
        ),
        BlockType::TodoItem => {
            let checked = block
                .properties
                .get("checked")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mark = if checked { "checked" } else { "" };
            format!(
                "<div class=\"todo\"><input type=\"checkbox\" {mark} disabled/> {}</div>",
                escape_html(block.content.as_str().unwrap_or(""))
            )
        }
        BlockType::Divider => "<hr/>".to_string(),
        BlockType::Image => {
            let alt = block
                .properties
                .get("alt")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!(
                "<img src=\"{}\" alt=\"{}\"/>",
                block.content.as_str().unwrap_or(""),
                escape_html(alt)
            )
        }
        BlockType::Table => {
            if let Some(rows) = block.content.as_array() {
                let mut html = String::from("<table>");
                for (i, row) in rows.iter().enumerate() {
                    html.push_str("<tr>");
                    if let Some(cells) = row.as_array() {
                        for c in cells {
                            let cell = escape_html(c.as_str().unwrap_or(""));
                            if i == 0 {
                                html.push_str(&format!("<th>{}</th>", cell));
                            } else {
                                html.push_str(&format!("<td>{}</td>", cell));
                            }
                        }
                    }
                    html.push_str("</tr>");
                }
                html.push_str("</table>");
                html
            } else {
                String::new()
            }
        }
        BlockType::Custom(_) => String::new(),
    }
}

// ============================================================================
// SubTask 3.4.4: 批量导入
// ============================================================================

/// 批量导入清单条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchManifestEntry {
    pub path: String,
    pub format: ImportFormat,
    pub size_bytes: u64,
    pub sha256: String,
}

/// 批量导入清单（manifest.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchImportManifest {
    pub version: u32,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub entries: Vec<BatchManifestEntry>,
}

impl BatchImportManifest {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            version: 1,
            source: source.into(),
            created_at: chrono::Utc::now(),
            entries: Vec::new(),
        }
    }

    pub fn add_entry(&mut self, entry: BatchManifestEntry) {
        self.entries.push(entry);
    }

    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size_bytes).sum()
    }
}

/// 导入进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProgress {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub current: Option<String>,
    pub completed_paths: Vec<String>,
    pub failed_paths: Vec<String>,
}

impl ImportProgress {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            done: 0,
            failed: 0,
            current: None,
            completed_paths: Vec::new(),
            failed_paths: Vec::new(),
        }
    }

    pub fn mark_done(&mut self, path: &str) {
        self.done += 1;
        self.completed_paths.push(path.to_string());
        self.current = None;
    }

    pub fn mark_failed(&mut self, path: &str) {
        self.failed += 1;
        self.failed_paths.push(path.to_string());
        self.current = None;
    }

    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            return 100.0;
        }
        ((self.done + self.failed) as f32 / self.total as f32) * 100.0
    }
}

/// 批量导入器（支持断点续传：根据已 completed_paths 跳过）
pub struct BatchImporter {
    state: Arc<RwLock<ImportProgress>>,
    manifest: BatchImportManifest,
}

impl BatchImporter {
    pub fn new(manifest: BatchImportManifest) -> Self {
        let total = manifest.entries.len();
        Self {
            state: Arc::new(RwLock::new(ImportProgress::new(total))),
            manifest,
        }
    }

    /// 从已有进度恢复（断点续传）
    pub fn resume(manifest: BatchImportManifest, progress: ImportProgress) -> Self {
        Self {
            state: Arc::new(RwLock::new(progress)),
            manifest,
        }
    }

    /// 执行导入（mock：直接将每条 entry 转换为空 Document 并标记完成）
    pub fn run<F>(&self, mut import_fn: F) -> ImportProgress
    where
        F: FnMut(&BatchManifestEntry) -> Result<Document, String>,
    {
        let completed: std::collections::HashSet<String> = {
            let s = self.state.read();
            s.completed_paths.iter().cloned().collect()
        };

        for entry in &self.manifest.entries {
            // 断点续传：跳过已完成的
            if completed.contains(&entry.path) {
                continue;
            }
            {
                let mut st = self.state.write();
                st.current = Some(entry.path.clone());
            }
            debug!(path = %entry.path, "importing entry");
            match import_fn(entry) {
                Ok(_doc) => {
                    self.state.write().mark_done(&entry.path);
                }
                Err(e) => {
                    info!(path = %entry.path, error = %e, "import failed");
                    self.state.write().mark_failed(&entry.path);
                }
            }
        }
        self.state.read().clone()
    }

    pub fn progress(&self) -> ImportProgress {
        self.state.read().clone()
    }
}

// ============================================================================
// 顶层 Importer / Exporter 门面
// ============================================================================

/// 导入器门面
pub struct Importer;

impl Importer {
    /// 从 Markdown 字符串导入为 Document
    pub fn import_markdown(markdown: &str) -> Document {
        let (fm, body) = Frontmatter::parse(markdown);
        let nodes = MarkdownParser::parse(&body);
        let title = fm
            .fields
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled")
            .to_string();
        nodes_to_document(&nodes, &title, &fm)
    }

    /// 从 Notion 块导入
    pub fn import_notion(title: &str, blocks: &[NotionBlock]) -> Document {
        NotionMigration::migrate(title, blocks)
    }

    /// 从 HTML 导入（简化：去除标签后按段落切分）
    pub fn import_html(html: &str) -> Document {
        let text = strip_html(html);
        let mut doc = Document::new("Imported from HTML");
        for para in text.split("\n\n") {
            let trimmed = para.trim();
            if !trimmed.is_empty() {
                doc = doc.with_block(Block::text(trimmed.to_string()));
            }
        }
        doc
    }
}

fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push('\n');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

/// 导出器门面
pub struct Exporter;

impl Exporter {
    /// 导出为 Markdown
    pub fn export_markdown(doc: &Document) -> String {
        let mut md = String::new();
        if !doc.properties.is_empty() {
            // 输出 frontmatter
            let fm = Frontmatter {
                fields: doc.properties.clone(),
            };
            let yaml = fm.to_yaml();
            if !yaml.is_empty() {
                md.push_str(&yaml);
                md.push('\n');
            }
        }
        md.push_str(&doc.to_markdown());
        md
    }

    /// 导出为 PDF（返回可打印 HTML）
    pub fn export_pdf(doc: &Document, config: PdfExportConfig) -> String {
        PdfExporter::new(config).render_html(doc)
    }

    /// 导出为 HTML
    pub fn export_html(doc: &Document) -> String {
        PdfExporter::new(PdfExportConfig::default()).render_html(doc)
    }

    /// 导出为 JSON
    pub fn export_json(doc: &Document) -> serde_json::Value {
        serde_json::to_value(doc).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Frontmatter ---

    #[test]
    fn test_frontmatter_parse() {
        let md = "---\ntitle: \"Hello\"\nauthor: \"Aurora\"\n---\n\n# Body";
        let (fm, body) = Frontmatter::parse(md);
        assert_eq!(fm.fields.get("title").unwrap().as_str().unwrap(), "Hello");
        assert_eq!(fm.fields.get("author").unwrap().as_str().unwrap(), "Aurora");
        assert!(body.starts_with("# Body"));
    }

    #[test]
    fn test_frontmatter_no_block() {
        let (fm, body) = Frontmatter::parse("# No frontmatter");
        assert!(fm.fields.is_empty());
        assert_eq!(body, "# No frontmatter");
    }

    #[test]
    fn test_frontmatter_to_yaml_roundtrip() {
        let mut fm = Frontmatter::default();
        fm.fields
            .insert("k".to_string(), serde_json::Value::String("v".to_string()));
        let yaml = fm.to_yaml();
        assert!(yaml.starts_with("---\n"));
        assert!(yaml.contains("k: \"v\""));
    }

    // --- Markdown parser ---

    #[test]
    fn test_md_parser_headings() {
        let nodes = MarkdownParser::parse("# H1\n## H2\n### H3");
        assert_eq!(nodes.len(), 3);
        for (i, n) in nodes.iter().enumerate() {
            if let MdNode::Heading { level, text } = n {
                assert_eq!(*level as usize, i + 1);
                assert_eq!(text, &format!("H{}", i + 1));
            } else {
                panic!("expected heading");
            }
        }
    }

    #[test]
    fn test_md_parser_code_fence() {
        let md = "```rust\nfn main() {}\n```";
        let nodes = MarkdownParser::parse(md);
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            MdNode::Code { language, code } => {
                assert_eq!(language, "rust");
                assert_eq!(code, "fn main() {}");
            }
            _ => panic!("expected code"),
        }
    }

    #[test]
    fn test_md_parser_lists_and_todos() {
        let md = "- item 1\n- [ ] todo\n- [x] done\n1. ordered";
        let nodes = MarkdownParser::parse(md);
        assert_eq!(nodes.len(), 4);
        assert!(matches!(nodes[0], MdNode::ListItem { ordered: false, .. }));
        assert!(matches!(nodes[1], MdNode::TodoItem { checked: false, .. }));
        assert!(matches!(nodes[2], MdNode::TodoItem { checked: true, .. }));
        assert!(matches!(nodes[3], MdNode::ListItem { ordered: true, .. }));
    }

    #[test]
    fn test_md_parser_quote_and_divider() {
        let md = "> line1\n> line2\n\n---";
        let nodes = MarkdownParser::parse(md);
        assert_eq!(nodes.len(), 2);
        if let MdNode::Quote { text } = &nodes[0] {
            assert_eq!(text, "line1\nline2");
        } else {
            panic!("expected quote");
        }
        assert!(matches!(nodes[1], MdNode::Divider));
    }

    #[test]
    fn test_md_parser_paragraph_merge() {
        let md = "first line\nsecond line\n\nthird para";
        let nodes = MarkdownParser::parse(md);
        assert_eq!(nodes.len(), 2);
        if let MdNode::Paragraph { text } = &nodes[0] {
            assert_eq!(text, "first line\nsecond line");
        } else {
            panic!("expected paragraph");
        }
    }

    // --- Markdown → Document round-trip ---

    #[test]
    fn test_markdown_import_roundtrip() {
        let md = "# Title\n\nSome paragraph text.\n\n- item 1\n- item 2";
        let doc = Importer::import_markdown(md);
        // 第一行是标题块（heading），其后是段落、两个列表项
        // 注：title 来自 frontmatter（缺省 "Untitled"），body 内含 # Title 块
        assert_eq!(doc.blocks.len(), 4);
        assert!(matches!(doc.blocks[0].block_type, BlockType::Heading));
        assert!(matches!(doc.blocks[1].block_type, BlockType::Text));
        assert!(matches!(doc.blocks[2].block_type, BlockType::ListItem));
        assert!(matches!(doc.blocks[3].block_type, BlockType::ListItem));
    }

    #[test]
    fn test_prosemirror_doc_structure() {
        let nodes = MarkdownParser::parse("# Hi");
        let doc = to_prosemirror_doc(&nodes, "Hi");
        assert_eq!(doc["type"], "doc");
        assert_eq!(doc["content"][0]["type"], "heading");
        assert_eq!(doc["content"][0]["attrs"]["level"], 1);
    }

    // --- Notion migration ---

    #[test]
    fn test_notion_block_mapping() {
        let nb = NotionBlock {
            id: "n1".into(),
            block_type: NotionBlockType::Heading2,
            rich_text: vec![NotionRichText {
                plain_text: "Title".into(),
                href: None,
                annotations: NotionAnnotations::default(),
            }],
            children: vec![],
            properties: HashMap::new(),
        };
        let block = nb.to_block();
        assert!(matches!(block.block_type, BlockType::Heading));
        let level = block
            .properties
            .get("level")
            .and_then(|v| v.as_u64())
            .unwrap();
        assert_eq!(level, 2);
    }

    #[test]
    fn test_notion_annotations_to_marks() {
        let nb = NotionBlock {
            id: "n1".into(),
            block_type: NotionBlockType::Paragraph,
            rich_text: vec![NotionRichText {
                plain_text: "bold text".into(),
                href: None,
                annotations: NotionAnnotations {
                    bold: true,
                    italic: true,
                    ..Default::default()
                },
            }],
            children: vec![],
            properties: HashMap::new(),
        };
        let block = nb.to_block();
        let marks = block.properties.get("marks").unwrap().as_array().unwrap();
        assert_eq!(marks.len(), 1);
        let mark_arr = marks[0]["marks"].as_array().unwrap();
        assert_eq!(mark_arr.len(), 2);
    }

    #[test]
    fn test_notion_migration_to_document() {
        let blocks = vec![
            NotionBlock {
                id: "1".into(),
                block_type: NotionBlockType::Heading1,
                rich_text: vec![NotionRichText {
                    plain_text: "Title".into(),
                    href: None,
                    annotations: NotionAnnotations::default(),
                }],
                children: vec![],
                properties: HashMap::new(),
            },
            NotionBlock {
                id: "2".into(),
                block_type: NotionBlockType::Paragraph,
                rich_text: vec![NotionRichText {
                    plain_text: "Body".into(),
                    href: None,
                    annotations: NotionAnnotations::default(),
                }],
                children: vec![],
                properties: HashMap::new(),
            },
        ];
        let doc = NotionMigration::migrate("My Doc", &blocks);
        assert_eq!(doc.title, "My Doc");
        assert_eq!(doc.blocks.len(), 2);
    }

    #[test]
    fn test_notion_collection_from_database() {
        let mut props = HashMap::new();
        props.insert("title".to_string(), serde_json::json!("My DB"));
        props.insert(
            "schema".to_string(),
            serde_json::json!({"Name": {"type": "title"}}),
        );
        let nb = NotionBlock {
            id: "db1".into(),
            block_type: NotionBlockType::ChildDatabase,
            rich_text: vec![],
            children: vec![],
            properties: props,
        };
        let col = NotionMigration::to_collection(&nb);
        assert_eq!(col.id, "db1");
        assert_eq!(col.title, "My DB");
        assert!(col.schema.contains_key("Name"));
    }

    // --- PDF export ---

    #[test]
    fn test_pdf_html_generation() {
        let mut doc = Document::new("Report");
        doc = doc.with_block(Block::heading(1, "Section"));
        doc = doc.with_block(Block::text("Content here"));
        let html = Exporter::export_pdf(&doc, PdfExportConfig::default());
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<h1>Report</h1>"));
        assert!(html.contains("Section"));
        assert!(html.contains("Content here"));
    }

    #[test]
    fn test_pdf_batch_render() {
        let d1 = Document::new("D1").with_block(Block::text("a"));
        let d2 = Document::new("D2").with_block(Block::text("b"));
        let exporter = PdfExporter::new(PdfExportConfig::default());
        let html = exporter.render_batch(&[d1, d2]);
        assert!(html.contains("D1"));
        assert!(html.contains("D2"));
        assert!(html.contains("page-break-after"));
    }

    #[test]
    fn test_pdf_template_page_size() {
        assert_eq!(PageSize::A4.mm(), (210.0, 297.0));
        assert_eq!(PageSize::Letter.mm().0, 215.9);
    }

    // --- Batch import ---

    #[test]
    fn test_batch_manifest() {
        let mut m = BatchImportManifest::new("notion-export");
        m.add_entry(BatchManifestEntry {
            path: "page1.md".into(),
            format: ImportFormat::Markdown,
            size_bytes: 100,
            sha256: "abc".into(),
        });
        m.add_entry(BatchManifestEntry {
            path: "page2.md".into(),
            format: ImportFormat::Markdown,
            size_bytes: 200,
            sha256: "def".into(),
        });
        assert_eq!(m.entries.len(), 2);
        assert_eq!(m.total_size(), 300);
        assert_eq!(m.version, 1);
    }

    #[test]
    fn test_batch_import_run() {
        let mut m = BatchImportManifest::new("test");
        m.add_entry(BatchManifestEntry {
            path: "a.md".into(),
            format: ImportFormat::Markdown,
            size_bytes: 10,
            sha256: "a".into(),
        });
        m.add_entry(BatchManifestEntry {
            path: "b.md".into(),
            format: ImportFormat::Markdown,
            size_bytes: 10,
            sha256: "b".into(),
        });
        let importer = BatchImporter::new(m);
        let progress = importer.run(|entry| {
            if entry.path == "b.md" {
                Err("simulated failure".to_string())
            } else {
                Ok(Document::new("ok"))
            }
        });
        assert_eq!(progress.done, 1);
        assert_eq!(progress.failed, 1);
        assert_eq!(progress.total, 2);
    }

    #[test]
    fn test_batch_import_resume() {
        let mut m = BatchImportManifest::new("test");
        m.add_entry(BatchManifestEntry {
            path: "a.md".into(),
            format: ImportFormat::Markdown,
            size_bytes: 10,
            sha256: "a".into(),
        });
        m.add_entry(BatchManifestEntry {
            path: "b.md".into(),
            format: ImportFormat::Markdown,
            size_bytes: 10,
            sha256: "b".into(),
        });
        let mut existing = ImportProgress::new(2);
        existing.mark_done("a.md");
        let importer = BatchImporter::resume(m, existing);
        let progress = importer.run(|_| Ok(Document::new("ok")));
        // a.md 已完成被跳过；只处理 b.md
        assert_eq!(progress.done, 2); // 1 已完成 + 1 新完成
        assert_eq!(progress.failed, 0);
    }

    #[test]
    fn test_import_progress_percent() {
        let mut p = ImportProgress::new(4);
        p.mark_done("a");
        p.mark_done("b");
        assert_eq!(p.percent(), 50.0);
        p.mark_failed("c");
        assert_eq!(p.percent(), 75.0);
    }

    // --- Exporter façade ---

    #[test]
    fn test_export_markdown_with_frontmatter() {
        let mut doc = Document::new("Title");
        doc.properties
            .insert("author".to_string(), serde_json::json!("Aurora"));
        doc = doc.with_block(Block::text("Hello"));
        let md = Exporter::export_markdown(&doc);
        assert!(md.starts_with("---\n"));
        assert!(md.contains("author: \"Aurora\""));
        assert!(md.contains("# Title"));
        assert!(md.contains("Hello"));
    }

    #[test]
    fn test_export_json_serialization() {
        let doc = Document::new("T").with_block(Block::text("body"));
        let json = Exporter::export_json(&doc);
        assert_eq!(json["title"], "T");
        assert_eq!(json["blocks"][0]["block_type"], "text");
    }

    #[test]
    fn test_import_html_strips_tags() {
        let html = "<h1>Title</h1><p>Para one</p><p>Para two</p>";
        let doc = Importer::import_html(html);
        assert!(doc.blocks.len() >= 2);
    }
}
