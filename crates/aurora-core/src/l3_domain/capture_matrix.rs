//! CaptureMatrix 捕获矩阵
//!
//! 实现网页剪藏、截图 OCR、语音速记、RSS 订阅四类「捕获入口」，
//! 将外部信息统一归一化为内部 `Document` / `Block` 结构。
//!
//! # 简化说明
//! - 网页剪藏的 Readability 算法为 **mock**：通过简单字符串扫描剥离
//!   `<script>/<style>/<nav>/<header>/<footer>` 等噪声标签，优先取
//!   `<article>` / `<main>` 作为正文，不做真实文本密度计算。
//! - HTML→Markdown 转换仅处理任务要求的子集标签（h1-h6 / p / ul-ol-li /
//!   code / pre / a / strong / em / blockquote），采用流式 emit 策略，
//!   不构建完整 DOM 树；属性解析不依赖 `html5ever` / `scraper`。
//! - 截图 OCR 复用 `crate::l3_domain::ocr_service::OcrEngine`，不重复实现
//!   双引擎；系统截图 API 与悬浮窗均为 mock 状态机。
//! - 语音 STT 的 `WhisperLocalProvider` / `CloudSttProvider` 均为 mock：
//!   基于音频字节的内容哈希派生确定性「识别结果」，真实 Whisper.cpp
//!   集成时替换 provider 实现即可。
//! - RSS 解析为简化 XML 扫描器（按 `<item>` 切片 + 子标签提取），
//!   不依赖 `feed-rs`；15 分钟轮询通过 `should_poll` 时间差判断。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{debug, info, warn};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use sha3::{Digest, Sha3_256};

use super::content_editor::{Block, Document};
use super::ocr_service::{BoundingBox, OcrEngine, OcrLanguage};
use super::asset_library::{Asset, AssetStore};

// ============================================================================
// SubTask 4.3.1: 网页剪藏 (Web Clipping)
// ============================================================================

/// 剪藏结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipResult {
    Success(ClippedPage),
    Failed { url: String, reason: String },
}

/// 剪藏后的页面
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClippedPage {
    pub url: String,
    pub title: String,
    pub content_html: String,
    pub content_markdown: String,
    pub excerpt: String,
    pub favicon: Option<String>,
    pub captured_at: DateTime<Utc>,
}

/// Readability 提取出的中间结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedPage {
    pub title: String,
    pub content_html: String,
    pub favicon: Option<String>,
}

/// Readability 提取器（mock）
///
/// 简化版「正文提取」：
/// 1. 剥离 `<script>/<style>/<nav>/<header>/<footer>/<noscript>` 整块
/// 2. 从 `<title>` 取标题
/// 3. 从 `<link rel="icon">` 取 favicon
/// 4. 优先取 `<article>` → `<main>` → 全文（剥除 `<head>` 后）作为正文
pub struct ReadabilityExtractor;

impl Default for ReadabilityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadabilityExtractor {
    pub fn new() -> Self {
        Self
    }

    pub fn extract(&self, url: &str, html: &str) -> ExtractedPage {
        let title = extract_tag_content(html, "title")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Untitled".to_string());

        let favicon = extract_favicon(html, url);

        let cleaned = strip_tag_blocks(
            html,
            &["script", "style", "nav", "header", "footer", "noscript", "iframe"],
        );

        // 剥除 <head>...</head>，避免 title/link 干扰正文
        let without_head = strip_tag_blocks(&cleaned, &["head"]);

        let content_html = extract_tag_content(&without_head, "article")
            .or_else(|| extract_tag_content(&without_head, "main"))
            .unwrap_or(without_head);

        debug!(url = %url, title = %title, "readability extracted (mock)");
        ExtractedPage {
            title,
            content_html,
            favicon,
        }
    }
}

/// 网页剪藏器：浏览器扩展 + Readability + Markdown 转换 + 消息传递
pub struct WebClipper {
    extractor: ReadabilityExtractor,
}

impl Default for WebClipper {
    fn default() -> Self {
        Self::new()
    }
}

impl WebClipper {
    pub fn new() -> Self {
        Self {
            extractor: ReadabilityExtractor::new(),
        }
    }

    /// 剪藏一个页面：提取正文 → 转 Markdown → 生成摘要
    pub fn clip(&self, url: &str, html: &str) -> ClipResult {
        if html.trim().is_empty() {
            warn!(url = %url, "clip failed: empty html");
            return ClipResult::Failed {
                url: url.to_string(),
                reason: "empty html".to_string(),
            };
        }
        let page = self.extractor.extract(url, html);
        let content_markdown = html_to_markdown(&page.content_html);
        let excerpt = make_excerpt(&content_markdown, 160);
        let clipped = ClippedPage {
            url: url.to_string(),
            title: page.title,
            content_html: page.content_html,
            content_markdown,
            excerpt,
            favicon: page.favicon,
            captured_at: Utc::now(),
        };
        info!(url = %url, "web page clipped");
        ClipResult::Success(clipped)
    }

    /// 将剪藏页面转换为内部 `Document`
    pub fn to_document(clipped: &ClippedPage) -> Document {
        let mut doc = Document::new(&clipped.title);
        doc.properties
            .insert("source_url".to_string(), serde_json::json!(clipped.url));
        doc.properties.insert(
            "captured_at".to_string(),
            serde_json::json!(clipped.captured_at.to_rfc3339()),
        );
        if let Some(ref favicon) = clipped.favicon {
            doc.properties
                .insert("favicon".to_string(), serde_json::json!(favicon));
        }
        for block in markdown_to_blocks(&clipped.content_markdown) {
            doc = doc.with_block(block);
        }
        doc
    }
}

// ---- HTML 工具函数（mock，不依赖外部 HTML 解析库） ----

/// HTML 词法 token
#[derive(Debug, Clone)]
enum HtmlToken {
    Text(String),
    OpenTag(String, HashMap<String, String>),
    CloseTag(String),
    SelfClosing(String, HashMap<String, String>),
}

/// 词法分析：将 HTML 切成 token 序列
fn tokenize_html(html: &str) -> Vec<HtmlToken> {
    let mut tokens = Vec::new();
    let bytes = html.as_bytes();
    let n = bytes.len();
    let mut i = 0usize;
    let mut text_start = 0usize;

    while i < n {
        if bytes[i] == b'<' {
            // flush text before tag
            if i > text_start {
                let text = &html[text_start..i];
                if !text.is_empty() {
                    tokens.push(HtmlToken::Text(text.to_string()));
                }
            }
            // find closing '>'
            if let Some(rel) = html[i..].find('>') {
                let raw = &html[i + 1..i + rel];
                let trimmed = raw.trim();
                if let Some(rest) = trimmed.strip_prefix('/') {
                    let name = rest.trim().to_lowercase();
                    tokens.push(HtmlToken::CloseTag(name));
                } else {
                    let self_closing = trimmed.ends_with('/');
                    let inner = if self_closing {
                        &trimmed[..trimmed.len() - 1]
                    } else {
                        trimmed
                    };
                    let (name, attrs) = parse_tag(inner);
                    if self_closing {
                        tokens.push(HtmlToken::SelfClosing(name, attrs));
                    } else {
                        tokens.push(HtmlToken::OpenTag(name, attrs));
                    }
                }
                i += rel + 1;
                text_start = i;
            } else {
                // malformed: no closing '>', treat '<' as text
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    if text_start < n {
        tokens.push(HtmlToken::Text(html[text_start..].to_string()));
    }
    tokens
}

/// 解析单个标签内部：`tag attr1="v1" attr2=v2`
fn parse_tag(s: &str) -> (String, HashMap<String, String>) {
    let mut parts = s.split_whitespace();
    let name = parts.next().unwrap_or("").to_lowercase();
    let attrs = parse_attrs(&s[name.len()..]);
    (name, attrs)
}

/// 解析属性串 `attr1="v1" attr2=v2 attr3`
fn parse_attrs(s: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut i = 0usize;
    while i < n {
        while i < n && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= n {
            break;
        }
        let name_start = i;
        while i < n && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let name = s[name_start..i].to_lowercase();
        while i < n && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < n && bytes[i] == b'=' {
            i += 1;
            while i < n && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < n && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                let val_start = i;
                while i < n && bytes[i] != quote {
                    i += 1;
                }
                attrs.insert(name, s[val_start..i].to_string());
                if i < n {
                    i += 1;
                }
            } else {
                let val_start = i;
                while i < n && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                attrs.insert(name, s[val_start..i].to_string());
            }
        } else if !name.is_empty() {
            attrs.insert(name, String::new());
        }
    }
    attrs
}

/// HTML 实体解码（处理常见实体）
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

/// 查找 `<tag ...>...</tag>` 的完整字节范围（含标签本身）
fn find_tag_block_range(html: &str, tag: &str) -> Option<(usize, usize)> {
    let open_prefix = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let mut search_from = 0usize;
    loop {
        let start = html[search_from..].find(&open_prefix)?;
        let abs_start = search_from + start;
        let after = &html[abs_start + open_prefix.len()..];
        let next_char = after.chars().next();
        match next_char {
            Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r') => {
                let open_end = html[abs_start..].find('>')?;
                let content_start = abs_start + open_end + 1;
                let close_rel = html[content_start..].find(&close)?;
                let block_end = content_start + close_rel + close.len();
                return Some((abs_start, block_end));
            }
            _ => {
                search_from = abs_start + 1;
            }
        }
    }
}

/// 提取 `<tag>content</tag>` 中的 content（取第一个匹配）
fn extract_tag_content(html: &str, tag: &str) -> Option<String> {
    let (start, end) = find_tag_block_range(html, tag)?;
    let open_end = start + html[start..].find('>')? + 1;
    let close_len = format!("</{}>", tag).len();
    Some(html[open_end..end - close_len].to_string())
}

/// 批量剥除指定标签块（含内容）
fn strip_tag_blocks(html: &str, tags: &[&str]) -> String {
    let mut result = html.to_string();
    for tag in tags {
        while let Some((start, end)) = find_tag_block_range(&result, tag) {
            result.replace_range(start..end, "");
        }
    }
    result
}

/// 从 `<link rel="icon" href="...">` 提取 favicon
fn extract_favicon(html: &str, base_url: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let mut search_from = 0usize;
    while let Some(pos) = lower[search_from..].find("<link") {
        let abs = search_from + pos;
        let end = html[abs..].find('>')?;
        let tag = &html[abs..abs + end + 1];
        let tag_lower = tag.to_lowercase();
        if tag_lower.contains("icon") {
            if let Some(href) = extract_attr(tag, "href") {
                return Some(resolve_url(&href, base_url));
            }
        }
        search_from = abs + 1;
    }
    None
}

/// 从标签字符串中提取属性值
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let pattern = format!("{}=", attr);
    let mut search_from = 0usize;
    while let Some(pos) = lower[search_from..].find(&pattern) {
        let abs = search_from + pos;
        let preceding = if abs == 0 {
            ' '
        } else {
            tag[..abs].chars().last().unwrap_or(' ')
        };
        if preceding.is_whitespace() || preceding == '<' {
            let val_start = abs + pattern.len();
            let bytes = tag.as_bytes();
            if val_start >= bytes.len() {
                return None;
            }
            let quote = bytes[val_start];
            if quote == b'"' || quote == b'\'' {
                let val = &tag[val_start + 1..];
                let end = val.find(quote as char)?;
                return Some(val[..end].to_string());
            } else {
                let val = &tag[val_start..];
                let end = val
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(val.len());
                return Some(val[..end].to_string());
            }
        }
        search_from = abs + 1;
    }
    None
}

/// 相对 URL 解析（简化版）
fn resolve_url(href: &str, base_url: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else if href.starts_with("//") {
        format!("https:{}", href)
    } else if href.starts_with('/') {
        if let Some(scheme_end) = base_url.find("://") {
            let host_start = scheme_end + 3;
            let host_end = base_url[host_start..]
                .find('/')
                .map(|e| host_start + e)
                .unwrap_or(base_url.len());
            return format!("{}{}", &base_url[..host_end], href);
        }
        href.to_string()
    } else {
        href.to_string()
    }
}

/// 列表上下文
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Ul,
    Ol,
}

/// HTML → Markdown 转换器（流式 emit）
struct MarkdownConverter {
    output: String,
    list_stack: Vec<(ListKind, u32)>,
    link_stack: Vec<String>,
    in_pre: bool,
}

impl MarkdownConverter {
    fn new() -> Self {
        Self {
            output: String::new(),
            list_stack: Vec::new(),
            link_stack: Vec::new(),
            in_pre: false,
        }
    }

    fn convert(tokens: Vec<HtmlToken>) -> String {
        let mut c = Self::new();
        for tok in tokens {
            c.process(tok);
        }
        c.output.trim_end().to_string()
    }

    fn ensure_newline(&mut self) {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn process(&mut self, tok: HtmlToken) {
        match tok {
            HtmlToken::Text(t) => {
                let decoded = decode_entities(&t);
                if self.in_pre {
                    self.output.push_str(&decoded);
                } else {
                    let collapsed: String =
                        decoded.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !collapsed.is_empty() {
                        self.output.push_str(&collapsed);
                    }
                }
            }
            HtmlToken::OpenTag(name, attrs) => self.open_tag(&name, &attrs),
            HtmlToken::CloseTag(name) => self.close_tag(&name),
            HtmlToken::SelfClosing(name, attrs) => match name.as_str() {
                "br" => self.output.push('\n'),
                "hr" => {
                    self.ensure_newline();
                    self.output.push_str("---\n");
                }
                "img" => {
                    let src = attrs.get("src").cloned().unwrap_or_default();
                    let alt = attrs.get("alt").cloned().unwrap_or_default();
                    self.output.push_str(&format!("![{}]({})", alt, src));
                }
                _ => {}
            },
        }
    }

    fn open_tag(&mut self, name: &str, attrs: &HashMap<String, String>) {
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = name[1..].parse::<usize>().unwrap_or(1);
                self.output.push_str(&"#".repeat(level));
                self.output.push(' ');
            }
            "p" => {}
            "ul" => self.list_stack.push((ListKind::Ul, 0)),
            "ol" => self.list_stack.push((ListKind::Ol, 0)),
            "li" => {
                if let Some(ctx) = self.list_stack.last_mut() {
                    ctx.1 += 1;
                    match ctx.0 {
                        ListKind::Ul => self.output.push_str("- "),
                        ListKind::Ol => self.output.push_str(&format!("{}. ", ctx.1)),
                    }
                }
            }
            "code" => {
                if !self.in_pre {
                    self.output.push('`');
                }
            }
            "pre" => {
                self.in_pre = true;
                self.ensure_newline();
                self.output.push_str("```\n");
            }
            "a" => {
                let href = attrs.get("href").cloned().unwrap_or_default();
                self.output.push('[');
                self.link_stack.push(href);
            }
            "strong" | "b" => self.output.push_str("**"),
            "em" | "i" => self.output.push('*'),
            "blockquote" => {
                self.ensure_newline();
                self.output.push_str("> ");
            }
            _ => {}
        }
    }

    fn close_tag(&mut self, name: &str) {
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.output.push_str("\n\n");
            }
            "p" => {
                self.output.push_str("\n\n");
            }
            "ul" | "ol" => {
                self.list_stack.pop();
                self.ensure_newline();
            }
            "li" => {
                self.ensure_newline();
            }
            "code" => {
                if !self.in_pre {
                    self.output.push('`');
                }
            }
            "pre" => {
                self.output.push_str("\n```");
                self.in_pre = false;
                self.output.push('\n');
            }
            "a" => {
                if let Some(href) = self.link_stack.pop() {
                    self.output.push_str(&format!("]({})", href));
                }
            }
            "strong" | "b" => self.output.push_str("**"),
            "em" | "i" => self.output.push('*'),
            "blockquote" => {
                self.output.push('\n');
            }
            _ => {}
        }
    }
}

/// HTML → Markdown 转换入口
pub fn html_to_markdown(html: &str) -> String {
    let tokens = tokenize_html(html);
    MarkdownConverter::convert(tokens)
}

/// 从 Markdown 生成摘要（去标记符号 + 截断）
fn make_excerpt(markdown: &str, max_chars: usize) -> String {
    let plain: String = markdown
        .chars()
        .filter(|c| !matches!(c, '#' | '*' | '`' | '>' | '|' | '\n'))
        .collect();
    let joined: String = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated: String = joined.chars().take(max_chars).collect();
    if joined.chars().count() > max_chars {
        format!("{}...", truncated)
    } else {
        truncated
    }
}

/// Markdown → Block 列表（按行扫描）
fn markdown_to_blocks(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        if let Some((level, text)) = parse_heading(trimmed) {
            blocks.push(Block::heading(level, text));
        } else if trimmed.starts_with("```") {
            let lang = trimmed[3..].trim();
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            let code = code_lines.join("\n");
            blocks.push(Block::code(
                if lang.is_empty() { "plaintext" } else { lang },
                code,
            ));
        } else if trimmed.starts_with("- ") {
            blocks.push(Block::list_item(trimmed[2..].trim()));
        } else if let Some(rest) = parse_ordered_item(trimmed) {
            blocks.push(Block::list_item(rest));
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            blocks.push(Block::quote(rest));
        } else if trimmed == "---" {
            blocks.push(Block::divider());
        } else {
            blocks.push(Block::text(trimmed));
        }
        i += 1;
    }
    blocks
}

fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let mut level = 0u8;
    for c in line.chars() {
        if c == '#' && level < 6 {
            level += 1;
        } else {
            break;
        }
    }
    if level == 0 {
        return None;
    }
    let rest = &line[level as usize..];
    if rest.is_empty() || rest.starts_with(' ') {
        Some((level, rest.trim()))
    } else {
        None
    }
}

fn parse_ordered_item(line: &str) -> Option<&str> {
    let dot_pos = line.find(". ")?;
    let prefix = &line[..dot_pos];
    if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
        Some(line[dot_pos + 2..].trim())
    } else {
        None
    }
}

// ============================================================================
// SubTask 4.3.2: 截图 OCR (Screenshot OCR)
// ============================================================================

/// 截图结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotResult {
    pub image_data: Vec<u8>,
    pub ocr_text: String,
    pub source_bounds: BoundingBox,
}

/// 悬浮窗内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FloatingContent {
    Screenshot(ScreenshotResult),
    OcrText(String),
    Empty,
}

/// 悬浮窗状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatingWindowState {
    pub visible: bool,
    pub position: (i32, i32),
    pub content: FloatingContent,
}

impl Default for FloatingWindowState {
    fn default() -> Self {
        Self {
            visible: false,
            position: (100, 100),
            content: FloatingContent::Empty,
        }
    }
}

/// 悬浮窗（mock 状态机：visible / position / content）
pub struct FloatingWindow {
    state: Arc<RwLock<FloatingWindowState>>,
}

impl Default for FloatingWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatingWindow {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(FloatingWindowState::default())),
        }
    }

    pub fn state(&self) -> FloatingWindowState {
        self.state.read().clone()
    }

    pub fn show(&self, content: FloatingContent) {
        let mut s = self.state.write();
        s.visible = true;
        s.content = content;
        debug!("floating window shown");
    }

    pub fn hide(&self) {
        self.state.write().visible = false;
        debug!("floating window hidden");
    }

    pub fn move_to(&self, x: i32, y: i32) {
        self.state.write().position = (x, y);
    }

    pub fn is_visible(&self) -> bool {
        self.state.read().visible
    }

    pub fn content(&self) -> FloatingContent {
        self.state.read().content.clone()
    }
}

/// 截图捕获器（mock 系统截图 API）
pub struct ScreenshotCapture {
    engine: Arc<OcrEngine>,
    last_capture: Arc<RwLock<Option<Vec<u8>>>>,
}

impl ScreenshotCapture {
    pub fn new(engine: Arc<OcrEngine>) -> Self {
        Self {
            engine,
            last_capture: Arc::new(RwLock::new(None)),
        }
    }

    /// mock 截图：基于区域生成确定性图像字节，再走 OCR 引擎识别
    pub fn capture(&self, region: Option<BoundingBox>) -> ScreenshotResult {
        let bounds = region.unwrap_or_default();
        let image_data = mock_capture_image(&bounds);
        let lines = self.engine.recognize(&image_data, OcrLanguage::Mixed);
        let ocr_text: String = lines
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        *self.last_capture.write() = Some(image_data.clone());
        info!(bytes = image_data.len(), "screenshot captured + ocr (mock)");
        ScreenshotResult {
            image_data,
            ocr_text,
            source_bounds: bounds,
        }
    }

    /// 将截图作为素材存入 AssetStore（内容寻址去重）
    pub fn store_as_asset(&self, store: &AssetStore, result: &ScreenshotResult) -> Asset {
        let asset = Asset::new("screenshot.png", "image/png", &result.image_data);
        let (stored, deduped) = store.put(asset);
        if deduped {
            debug!("screenshot asset deduplicated");
        }
        stored
    }

    pub fn last_capture(&self) -> Option<Vec<u8>> {
        self.last_capture.read().clone()
    }
}

/// 截图 OCR 工作流：全局快捷键 → 截图 → OCR → 悬浮窗 → 转为笔记
pub struct ScreenshotOcrWorkflow {
    pub capture: ScreenshotCapture,
    pub window: FloatingWindow,
}

impl ScreenshotOcrWorkflow {
    pub fn new(engine: Arc<OcrEngine>) -> Self {
        Self {
            capture: ScreenshotCapture::new(engine),
            window: FloatingWindow::new(),
        }
    }

    /// 触发截图并识别，结果展示到悬浮窗
    pub fn capture_and_recognize(&self, region: Option<BoundingBox>) -> ScreenshotResult {
        let result = self.capture.capture(region);
        self.window
            .show(FloatingContent::Screenshot(result.clone()));
        result
    }

    /// 「截图即笔记」：将悬浮窗中的截图结果转为 `Document`
    pub fn to_note(&self) -> Document {
        let content = self.window.content();
        match content {
            FloatingContent::Screenshot(result) => screenshot_to_document(&result),
            FloatingContent::OcrText(text) => {
                let mut doc = Document::new("Screenshot Note");
                doc = doc.with_block(Block::text(text));
                doc
            }
            FloatingContent::Empty => Document::new("Empty Screenshot"),
        }
    }
}

/// 截图结果 → Document
fn screenshot_to_document(result: &ScreenshotResult) -> Document {
    let mut doc = Document::new("Screenshot Note");
    doc.properties.insert(
        "captured_bounds".to_string(),
        serde_json::json!({
            "x": result.source_bounds.x,
            "y": result.source_bounds.y,
            "width": result.source_bounds.width,
            "height": result.source_bounds.height,
        }),
    );
    if result.ocr_text.is_empty() {
        doc = doc.with_block(Block::text("(no text recognized)"));
    } else {
        for line in result.ocr_text.lines() {
            doc = doc.with_block(Block::text(line));
        }
    }
    doc
}

/// mock 截图图像生成：基于区域派生确定性字节
fn mock_capture_image(bounds: &BoundingBox) -> Vec<u8> {
    let seed = bounds.x as u64
        ^ ((bounds.y as u64) << 16)
        ^ ((bounds.width as u64) << 32)
        ^ ((bounds.height as u64) << 48);
    let area = (bounds.width as usize) * (bounds.height as usize);
    let size = area.max(256).min(4096);
    (0..size)
        .map(|i| ((seed.wrapping_add(i as u64)) & 0xFF) as u8)
        .collect()
}

// ============================================================================
// SubTask 4.3.3: 语音速记 (Voice Memo)
// ============================================================================

/// 语音转写片段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub start: f32,
    pub end: f32,
    pub text: String,
    pub confidence: f32,
}

/// 转写结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub segments: Vec<TranscriptionSegment>,
    pub confidence: f32,
    pub language: String,
}

/// STT Provider Trait
pub trait SttProvider: Send + Sync {
    fn name(&self) -> &str;
    fn transcribe(&self, audio_data: &[u8], language: &str) -> TranscriptionResult;
}

/// Whisper.cpp 本地 STT（mock）
pub struct WhisperLocalProvider;

impl SttProvider for WhisperLocalProvider {
    fn name(&self) -> &str {
        "whisper-local"
    }

    fn transcribe(&self, audio_data: &[u8], language: &str) -> TranscriptionResult {
        mock_transcribe(audio_data, language, "whisper")
    }
}

/// 云端 STT（mock）
pub struct CloudSttProvider {
    pub api_endpoint: String,
}

impl SttProvider for CloudSttProvider {
    fn name(&self) -> &str {
        "cloud-stt"
    }

    fn transcribe(&self, audio_data: &[u8], language: &str) -> TranscriptionResult {
        mock_transcribe(audio_data, language, "cloud")
    }
}

/// 录音器状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecorderState {
    pub recording: bool,
    pub duration_seconds: f32,
    pub samples: u64,
}

impl Default for RecorderState {
    fn default() -> Self {
        Self {
            recording: false,
            duration_seconds: 0.0,
            samples: 0,
        }
    }
}

/// 语音录音器（mock 状态机：recording / duration / samples）
pub struct VoiceRecorder {
    state: Arc<RwLock<RecorderState>>,
}

impl Default for VoiceRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceRecorder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(RecorderState::default())),
        }
    }

    pub fn state(&self) -> RecorderState {
        self.state.read().clone()
    }

    pub fn start(&self) {
        let mut s = self.state.write();
        s.recording = true;
        s.duration_seconds = 0.0;
        s.samples = 0;
        debug!("voice recorder started");
    }

    /// 停止录音，返回 mock 音频字节
    pub fn stop(&self) -> Vec<u8> {
        let mut s = self.state.write();
        s.recording = false;
        let duration = s.duration_seconds;
        let samples = s.samples;
        // mock 音频：基于 duration 派生确定性字节
        let mut data = Vec::new();
        let seed = (duration.to_bits() as u64) ^ (samples << 32);
        for i in 0..((duration * 16000.0) as usize).max(64) {
            data.push(((seed.wrapping_add(i as u64)) & 0xFF) as u8);
        }
        debug!(duration, samples, "voice recorder stopped");
        data
    }

    /// 推进录音（mock：按秒递增 duration 与采样数）
    pub fn tick(&self, seconds: f32) {
        let mut s = self.state.write();
        if s.recording {
            s.duration_seconds += seconds;
            s.samples += (seconds * 16000.0) as u64;
        }
    }

    pub fn is_recording(&self) -> bool {
        self.state.read().recording
    }
}

/// 实时转写器：将转写片段实时插入文档
pub struct RealtimeTranscriber {
    segments: Arc<RwLock<Vec<TranscriptionSegment>>>,
    document: Arc<RwLock<Document>>,
}

impl Default for RealtimeTranscriber {
    fn default() -> Self {
        Self::new()
    }
}

impl RealtimeTranscriber {
    pub fn new() -> Self {
        Self {
            segments: Arc::new(RwLock::new(Vec::new())),
            document: Arc::new(RwLock::new(Document::new("Voice Memo"))),
        }
    }

    /// 插入一个转写片段，同时追加为文档 Block
    pub fn insert_segment(&self, segment: TranscriptionSegment) -> Block {
        let block = Block::text(segment.text.clone());
        {
            let mut doc = self.document.write();
            doc.blocks.push(block.clone());
            doc.updated_at = Utc::now();
            doc.version += 1;
        }
        self.segments.write().push(segment);
        block
    }

    pub fn segments(&self) -> Vec<TranscriptionSegment> {
        self.segments.read().clone()
    }

    pub fn document(&self) -> Document {
        self.document.read().clone()
    }

    pub fn segment_count(&self) -> usize {
        self.segments.read().len()
    }
}

/// 语音速记入口
pub struct VoiceMemo {
    pub recorder: VoiceRecorder,
    provider: Arc<dyn SttProvider>,
    pub realtime: RealtimeTranscriber,
    language: String,
}

impl VoiceMemo {
    pub fn new(provider: Arc<dyn SttProvider>) -> Self {
        Self {
            recorder: VoiceRecorder::new(),
            provider,
            realtime: RealtimeTranscriber::new(),
            language: "zh".to_string(),
        }
    }

    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = lang.into();
        self
    }

    /// 开始录音
    pub fn start_recording(&self) {
        self.recorder.start();
    }

    /// 推进录音
    pub fn tick(&self, seconds: f32) {
        self.recorder.tick(seconds);
    }

    /// 停止录音并转写
    pub fn stop_and_transcribe(&self) -> TranscriptionResult {
        let audio = self.recorder.stop();
        let result = self
            .provider
            .transcribe(&audio, &self.language);
        for seg in result.segments.clone() {
            self.realtime.insert_segment(seg);
        }
        info!(segments = result.segments.len(), "voice memo transcribed");
        result
    }

    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }
}

/// mock STT：基于音频内容哈希派生确定性转写结果
fn mock_transcribe(audio_data: &[u8], language: &str, provider: &str) -> TranscriptionResult {
    if audio_data.is_empty() {
        return TranscriptionResult {
            text: String::new(),
            segments: Vec::new(),
            confidence: 0.0,
            language: language.to_string(),
        };
    }
    let hash = audio_hash(audio_data);
    let seg_count = (hash % 5) as usize + 1;
    let segments: Vec<TranscriptionSegment> = (0..seg_count)
        .map(|i| {
            let line_hash = hash.wrapping_mul((i + 1) as u64);
            let start = i as f32 * 2.0;
            TranscriptionSegment {
                start,
                end: start + 2.0,
                text: format!("[{}] segment-{} {:08x}", provider, i + 1, line_hash),
                confidence: 0.80 + (line_hash % 20) as f32 / 100.0,
            }
        })
        .collect();
    let text = segments
        .iter()
        .map(|s| s.text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    let confidence = segments.iter().map(|s| s.confidence).sum::<f32>() / segments.len() as f32;
    TranscriptionResult {
        text,
        segments,
        confidence,
        language: language.to_string(),
    }
}

/// 音频内容哈希（取 SHA3-256 低 64 位）
fn audio_hash(data: &[u8]) -> u64 {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash: u64 = 0;
    for i in 0..8.min(result.len()) {
        hash |= (result[i] as u64) << (i * 8);
    }
    hash
}

// ============================================================================
// SubTask 4.3.4: RSS 订阅 (RSS Subscription)
// ============================================================================

/// RSS 订阅配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssSubscription {
    pub id: String,
    pub feed_url: String,
    pub title: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

impl RssSubscription {
    pub fn new(feed_url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            feed_url: feed_url.into(),
            title: None,
            enabled: true,
            created_at: Utc::now(),
        }
    }
}

/// RSS Feed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssFeed {
    pub url: String,
    pub title: String,
    pub items: Vec<RssItem>,
}

/// RSS 条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssItem {
    pub guid: String,
    pub title: String,
    pub link: String,
    pub published: Option<DateTime<Utc>>,
    pub summary: String,
    pub content: String,
}

/// RSS 解析器（简化 XML 扫描）
pub struct RssParser;

impl RssParser {
    /// 解析 RSS XML，返回 RssFeed
    pub fn parse(url: &str, xml: &str) -> RssFeed {
        let channel_title = extract_first_tag(xml, "title")
            .unwrap_or_else(|| "Untitled Feed".to_string());
        let items = extract_rss_items(xml);
        RssFeed {
            url: url.to_string(),
            title: channel_title,
            items,
        }
    }
}

/// 从 XML 中提取第一个 `<tag>content</tag>` 的 content
fn extract_first_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start_rel = xml.find(&open)?;
    let open_end = xml[start_rel..].find('>')?;
    let content_start = start_rel + open_end + 1;
    let close_rel = xml[content_start..].find(&close)?;
    let raw = &xml[content_start..content_start + close_rel];
    Some(decode_entities(raw).trim().to_string())
}

/// 提取所有 `<item>...</item>` 块
fn extract_rss_items(xml: &str) -> Vec<RssItem> {
    let mut items = Vec::new();
    let mut search_from = 0usize;
    while let Some(rel) = xml[search_from..].find("<item>") {
        let abs = search_from + rel + 6;
        let close = match xml[abs..].find("</item>") {
            Some(c) => abs + c,
            None => break,
        };
        let item_xml = &xml[abs..close];
        let title = extract_first_tag(item_xml, "title").unwrap_or_default();
        let link = extract_first_tag(item_xml, "link").unwrap_or_default();
        let published = extract_first_tag(item_xml, "pubDate")
            .and_then(|s| parse_rss_date(&s));
        let description = extract_first_tag(item_xml, "description").unwrap_or_default();
        let content = extract_first_tag(item_xml, "content:encoded")
            .or_else(|| extract_first_tag(item_xml, "content"))
            .unwrap_or_else(|| description.clone());
        let guid = extract_first_tag(item_xml, "guid")
            .unwrap_or_else(|| link.clone());
        items.push(RssItem {
            guid,
            title,
            link,
            published,
            summary: description,
            content,
        });
        search_from = close + 7;
    }
    items
}

/// 解析 RFC 2822 日期（RSS pubDate 标准格式）
fn parse_rss_date(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(s.trim())
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// RSS 轮询器：15 分钟间隔 + last_poll 跟踪 + 新条目检测
pub struct RssPoller {
    interval_seconds: u64,
    subscriptions: Arc<RwLock<Vec<RssSubscription>>>,
    last_poll: Arc<RwLock<Option<DateTime<Utc>>>>,
    seen_items: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    fetched_feeds: Arc<RwLock<Vec<RssFeed>>>,
}

impl Default for RssPoller {
    fn default() -> Self {
        Self::new()
    }
}

impl RssPoller {
    pub fn new() -> Self {
        Self::with_interval(900) // 15 分钟
    }

    pub fn with_interval(seconds: u64) -> Self {
        Self {
            interval_seconds: seconds,
            subscriptions: Arc::new(RwLock::new(Vec::new())),
            last_poll: Arc::new(RwLock::new(None)),
            seen_items: Arc::new(RwLock::new(HashMap::new())),
            fetched_feeds: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn interval_seconds(&self) -> u64 {
        self.interval_seconds
    }

    pub fn add_subscription(&self, sub: RssSubscription) {
        self.subscriptions.write().push(sub);
    }

    pub fn subscription_count(&self) -> usize {
        self.subscriptions.read().len()
    }

    pub fn subscriptions(&self) -> Vec<RssSubscription> {
        self.subscriptions.read().clone()
    }

    pub fn last_poll(&self) -> Option<DateTime<Utc>> {
        *self.last_poll.read()
    }

    /// 判断是否到达轮询时间
    pub fn should_poll(&self, now: DateTime<Utc>) -> bool {
        match *self.last_poll.read() {
            None => true,
            Some(last) => (now - last).num_seconds() >= self.interval_seconds as i64,
        }
    }

    /// 标记已轮询
    pub fn mark_polled(&self, now: DateTime<Utc>) {
        *self.last_poll.write() = Some(now);
    }

    /// 轮询一个订阅：解析 feed，返回此前未见的新条目
    pub fn poll_feed(&self, sub: &RssSubscription, feed_xml: &str) -> Vec<RssItem> {
        let feed = RssParser::parse(&sub.feed_url, feed_xml);
        let new_items = {
            let mut seen = self.seen_items.write();
            let sub_seen = seen.entry(sub.id.clone()).or_insert_with(HashSet::new);
            let new: Vec<RssItem> = feed
                .items
                .iter()
                .filter(|item| !sub_seen.contains(&item.guid))
                .cloned()
                .collect();
            for item in &new {
                sub_seen.insert(item.guid.clone());
            }
            new
        };
        self.fetched_feeds.write().push(feed);
        info!(sub_id = %sub.id, new_count = new_items.len(), "rss feed polled");
        new_items
    }

    pub fn fetched_feeds(&self) -> Vec<RssFeed> {
        self.fetched_feeds.read().clone()
    }

    /// RSS 条目 → Document
    pub fn item_to_document(item: &RssItem) -> Document {
        let mut doc = Document::new(&item.title);
        doc.properties
            .insert("source_link".to_string(), serde_json::json!(item.link));
        doc.properties
            .insert("guid".to_string(), serde_json::json!(item.guid));
        if let Some(pub_at) = item.published {
            doc.properties
                .insert("published".to_string(), serde_json::json!(pub_at.to_rfc3339()));
        }
        if !item.summary.is_empty() {
            doc = doc.with_block(Block::quote(&item.summary));
        }
        if !item.content.is_empty() && item.content != item.summary {
            for line in item.content.lines() {
                let t = line.trim();
                if !t.is_empty() {
                    doc = doc.with_block(Block::text(t));
                }
            }
        }
        doc
    }
}

// ============================================================================
// 顶层 CaptureMatrix 捕获矩阵
// ============================================================================

/// CaptureMatrix 顶层协调器：聚合四类捕获入口
pub struct CaptureMatrix {
    pub web_clipper: WebClipper,
    pub screenshot: ScreenshotOcrWorkflow,
    pub voice: VoiceMemo,
    pub rss: RssPoller,
}

impl Default for CaptureMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureMatrix {
    pub fn new() -> Self {
        let ocr_engine = Arc::new(OcrEngine::new());
        Self {
            web_clipper: WebClipper::new(),
            screenshot: ScreenshotOcrWorkflow::new(ocr_engine),
            voice: VoiceMemo::new(Arc::new(WhisperLocalProvider)),
            rss: RssPoller::new(),
        }
    }

    /// 路由：网页剪藏
    pub fn clip_web(&self, url: &str, html: &str) -> ClipResult {
        self.web_clipper.clip(url, html)
    }

    /// 路由：截图 OCR
    pub fn capture_screenshot(&self, region: Option<BoundingBox>) -> ScreenshotResult {
        self.screenshot.capture_and_recognize(region)
    }

    /// 路由：语音速记
    pub fn record_voice(&self, seconds: f32) -> TranscriptionResult {
        self.voice.start_recording();
        self.voice.tick(seconds);
        self.voice.stop_and_transcribe()
    }

    /// 路由：RSS 轮询
    pub fn poll_rss(&self, sub: &RssSubscription, xml: &str) -> Vec<RssItem> {
        self.rss.poll_feed(sub, xml)
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::content_editor::BlockType;
    use chrono::Duration;

    // ---- Readability 提取 ----

    fn sample_html() -> &'static str {
        r#"<!DOCTYPE html>
<html>
<head>
  <title>Test Page Title</title>
  <link rel="icon" href="/favicon.ico">
  <link rel="stylesheet" href="/style.css">
  <script>alert("bad");</script>
</head>
<body>
  <nav><a href="/">Home</a></nav>
  <header><h1>Site Header</h1></header>
  <article>
    <h2>Article Heading</h2>
    <p>This is the <strong>main</strong> article content.</p>
    <p>Second paragraph with a <a href="https://example.com">link</a>.</p>
  </article>
  <footer>Copyright 2024</footer>
</body>
</html>"#
    }

    #[test]
    fn test_readability_strips_script_style_nav() {
        let ext = ReadabilityExtractor::new();
        let page = ext.extract("https://example.com", sample_html());
        assert!(!page.content_html.contains("alert"));
        assert!(!page.content_html.contains("Home"));
        assert!(!page.content_html.contains("Copyright"));
        assert!(page.content_html.contains("article content"));
        assert!(page.content_html.contains("Article Heading"));
    }

    #[test]
    fn test_readability_extracts_title() {
        let ext = ReadabilityExtractor::new();
        let page = ext.extract("https://example.com", sample_html());
        assert_eq!(page.title, "Test Page Title");
    }

    #[test]
    fn test_readability_extracts_favicon() {
        let ext = ReadabilityExtractor::new();
        let page = ext.extract("https://example.com", sample_html());
        assert_eq!(page.favicon.as_deref(), Some("https://example.com/favicon.ico"));
    }

    #[test]
    fn test_readability_prefers_article_content() {
        let ext = ReadabilityExtractor::new();
        let page = ext.extract("https://example.com", sample_html());
        assert!(page.content_html.contains("Article Heading"));
        assert!(page.content_html.contains("Second paragraph"));
    }

    #[test]
    fn test_readability_falls_back_to_main() {
        let html = r#"<html><body><main><p>main content</p></main></body></html>"#;
        let ext = ReadabilityExtractor::new();
        let page = ext.extract("https://x.com", html);
        assert!(page.content_html.contains("main content"));
    }

    #[test]
    fn test_readability_falls_back_to_body() {
        let html = r#"<html><body><p>just body text</p></body></html>"#;
        let ext = ReadabilityExtractor::new();
        let page = ext.extract("https://x.com", html);
        assert!(page.content_html.contains("just body text"));
    }

    #[test]
    fn test_readability_title_default_when_missing() {
        let html = r#"<html><body><p>no title here</p></body></html>"#;
        let ext = ReadabilityExtractor::new();
        let page = ext.extract("https://x.com", html);
        assert_eq!(page.title, "Untitled");
    }

    // ---- HTML → Markdown 转换 ----

    #[test]
    fn test_html_to_markdown_headings() {
        let md = html_to_markdown("<h1>Title</h1><h2>Sub</h2><h3>SubSub</h3>");
        assert!(md.contains("# Title"));
        assert!(md.contains("## Sub"));
        assert!(md.contains("### SubSub"));
    }

    #[test]
    fn test_html_to_markdown_paragraph() {
        let md = html_to_markdown("<p>Hello world</p>");
        assert!(md.contains("Hello world"));
    }

    #[test]
    fn test_html_to_markdown_unordered_list() {
        let md = html_to_markdown("<ul><li>apple</li><li>banana</li></ul>");
        assert!(md.contains("- apple"));
        assert!(md.contains("- banana"));
    }

    #[test]
    fn test_html_to_markdown_ordered_list() {
        let md = html_to_markdown("<ol><li>first</li><li>second</li></ol>");
        assert!(md.contains("1. first"));
        assert!(md.contains("2. second"));
    }

    #[test]
    fn test_html_to_markdown_inline_code() {
        let md = html_to_markdown("<p>use <code>println!</code> here</p>");
        assert!(md.contains("`println!`"));
    }

    #[test]
    fn test_html_to_markdown_pre_block() {
        let md = html_to_markdown("<pre><code>fn main() {}
}</code></pre>");
        assert!(md.contains("```"));
        assert!(md.contains("fn main()"));
    }

    #[test]
    fn test_html_to_markdown_link() {
        let md = html_to_markdown(r#"<a href="https://example.com">click here</a>"#);
        assert!(md.contains("[click here](https://example.com)"));
    }

    #[test]
    fn test_html_to_markdown_strong_and_em() {
        let md = html_to_markdown("<p><strong>bold</strong> and <em>italic</em></p>");
        assert!(md.contains("**bold**"));
        assert!(md.contains("*italic*"));
    }

    #[test]
    fn test_html_to_markdown_blockquote() {
        let md = html_to_markdown("<blockquote>a wise quote</blockquote>");
        assert!(md.contains("> a wise quote"));
    }

    #[test]
    fn test_html_to_markdown_decodes_entities() {
        let md = html_to_markdown("<p>Tom &amp; Jerry &lt;3</p>");
        assert!(md.contains("Tom & Jerry"));
        assert!(md.contains("<3"));
    }

    #[test]
    fn test_html_to_markdown_nested_structure() {
        let md = html_to_markdown(
            r#"<article><h1>Post</h1><p>Intro</p><ul><li>a</li><li>b</li></ul></article>"#,
        );
        assert!(md.contains("# Post"));
        assert!(md.contains("Intro"));
        assert!(md.contains("- a"));
        assert!(md.contains("- b"));
    }

    // ---- WebClipper ----

    #[test]
    fn test_web_clipper_clip_success() {
        let clipper = WebClipper::new();
        let result = clipper.clip("https://example.com", sample_html());
        match result {
            ClipResult::Success(page) => {
                assert_eq!(page.url, "https://example.com");
                assert_eq!(page.title, "Test Page Title");
                assert!(page.content_markdown.contains("Article Heading"));
                assert!(!page.excerpt.is_empty());
                assert!(page.favicon.is_some());
            }
            ClipResult::Failed { .. } => panic!("expected success"),
        }
    }

    #[test]
    fn test_web_clipper_clip_empty_html() {
        let clipper = WebClipper::new();
        let result = clipper.clip("https://example.com", "");
        assert!(matches!(result, ClipResult::Failed { .. }));
    }

    #[test]
    fn test_web_clipper_to_document() {
        let clipper = WebClipper::new();
        let page = match clipper.clip("https://example.com", sample_html()) {
            ClipResult::Success(p) => p,
            _ => panic!("expected success"),
        };
        let doc = WebClipper::to_document(&page);
        assert_eq!(doc.title, "Test Page Title");
        assert_eq!(
            doc.properties.get("source_url").unwrap().as_str().unwrap(),
            "https://example.com"
        );
        assert!(!doc.blocks.is_empty());
        // 第一个块应为 heading（Article Heading → ## Article Heading）
        assert!(matches!(doc.blocks[0].block_type, BlockType::Heading));
    }

    #[test]
    fn test_web_clipper_excerpt_truncates() {
        let long_html = format!(
            "<article><p>{}</p></article>",
            "word ".repeat(200)
        );
        let clipper = WebClipper::new();
        let page = match clipper.clip("https://x.com", &long_html) {
            ClipResult::Success(p) => p,
            _ => panic!("expected success"),
        };
        assert!(page.excerpt.len() <= 163); // 160 + "..."
        assert!(page.excerpt.ends_with("..."));
    }

    // ---- Screenshot OCR ----

    #[test]
    fn test_screenshot_capture_returns_result() {
        let engine = Arc::new(OcrEngine::new());
        let cap = ScreenshotCapture::new(engine);
        let result = cap.capture(Some(BoundingBox {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        }));
        assert!(!result.image_data.is_empty());
        assert_eq!(result.source_bounds.width, 100);
        assert!(cap.last_capture().is_some());
    }

    #[test]
    fn test_screenshot_capture_ocr_text() {
        let engine = Arc::new(OcrEngine::new());
        let cap = ScreenshotCapture::new(engine);
        let result = cap.capture(Some(BoundingBox {
            x: 10,
            y: 20,
            width: 200,
            height: 150,
        }));
        // OCR 引擎 mock 返回非空文本
        assert!(!result.ocr_text.is_empty());
    }

    #[test]
    fn test_floating_window_state_transitions() {
        let win = FloatingWindow::new();
        assert!(!win.is_visible());
        win.show(FloatingContent::OcrText("hello".to_string()));
        assert!(win.is_visible());
        assert!(matches!(win.content(), FloatingContent::OcrText(_)));
        win.move_to(50, 60);
        assert_eq!(win.state().position, (50, 60));
        win.hide();
        assert!(!win.is_visible());
    }

    #[test]
    fn test_screenshot_workflow_capture_and_show() {
        let engine = Arc::new(OcrEngine::new());
        let wf = ScreenshotOcrWorkflow::new(engine);
        let result = wf.capture_and_recognize(Some(BoundingBox::default()));
        assert!(wf.window.is_visible());
        match wf.window.content() {
            FloatingContent::Screenshot(r) => assert_eq!(r.ocr_text, result.ocr_text),
            _ => panic!("expected screenshot content"),
        }
    }

    #[test]
    fn test_screenshot_to_note_document() {
        let engine = Arc::new(OcrEngine::new());
        let wf = ScreenshotOcrWorkflow::new(engine);
        wf.capture_and_recognize(Some(BoundingBox {
            x: 1,
            y: 2,
            width: 50,
            height: 50,
        }));
        let doc = wf.to_note();
        assert_eq!(doc.title, "Screenshot Note");
        assert!(!doc.blocks.is_empty());
        assert!(doc.properties.contains_key("captured_bounds"));
    }

    #[test]
    fn test_screenshot_store_as_asset() {
        let engine = Arc::new(OcrEngine::new());
        let cap = ScreenshotCapture::new(engine);
        let store = AssetStore::new();
        let result = cap.capture(Some(BoundingBox::default()));
        let asset = cap.store_as_asset(&store, &result);
        assert_eq!(store.count(), 1);
        assert_eq!(asset.mime_type, "image/png");
        // 再次存储相同内容应去重
        let _deduped = cap.store_as_asset(&store, &result);
        assert_eq!(store.count(), 1);
    }

    // ---- Voice Memo ----

    #[test]
    fn test_voice_recorder_start_stop() {
        let rec = VoiceRecorder::new();
        assert!(!rec.is_recording());
        rec.start();
        assert!(rec.is_recording());
        let audio = rec.stop();
        assert!(!rec.is_recording());
        assert!(!audio.is_empty()); // mock 至少 64 字节
    }

    #[test]
    fn test_voice_recorder_tick_advances_duration() {
        let rec = VoiceRecorder::new();
        rec.start();
        rec.tick(2.5);
        let state = rec.state();
        assert!((state.duration_seconds - 2.5).abs() < 0.01);
        assert_eq!(state.samples, (2.5 * 16000.0) as u64);
        rec.stop();
    }

    #[test]
    fn test_voice_recorder_tick_ignored_when_stopped() {
        let rec = VoiceRecorder::new();
        rec.tick(5.0);
        assert_eq!(rec.state().duration_seconds, 0.0);
    }

    #[test]
    fn test_whisper_local_provider_transcribe() {
        let p = WhisperLocalProvider;
        let result = p.transcribe(&[1, 2, 3, 4, 5], "zh");
        assert!(!result.text.is_empty());
        assert!(!result.segments.is_empty());
        assert!(result.segments.iter().all(|s| s.text.contains("[whisper]")));
        assert!(result.confidence > 0.0);
        assert_eq!(result.language, "zh");
    }

    #[test]
    fn test_cloud_stt_provider_transcribe() {
        let p = CloudSttProvider {
            api_endpoint: "https://stt.example.com".to_string(),
        };
        let result = p.transcribe(&[10, 20, 30], "en");
        assert!(!result.segments.is_empty());
        assert!(result.segments.iter().all(|s| s.text.contains("[cloud]")));
        assert_eq!(result.language, "en");
    }

    #[test]
    fn test_stt_empty_audio_returns_empty() {
        let p = WhisperLocalProvider;
        let result = p.transcribe(&[], "zh");
        assert!(result.text.is_empty());
        assert!(result.segments.is_empty());
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_stt_deterministic_same_input() {
        let p = WhisperLocalProvider;
        let a = p.transcribe(&[1, 2, 3, 4, 5], "zh");
        let b = p.transcribe(&[1, 2, 3, 4, 5], "zh");
        assert_eq!(a.text, b.text);
        assert_eq!(a.segments.len(), b.segments.len());
    }

    #[test]
    fn test_realtime_transcriber_insert_segment() {
        let rt = RealtimeTranscriber::new();
        let seg = TranscriptionSegment {
            start: 0.0,
            end: 2.0,
            text: "hello world".to_string(),
            confidence: 0.95,
        };
        let block = rt.insert_segment(seg);
        assert!(matches!(block.block_type, BlockType::Text));
        assert_eq!(rt.segment_count(), 1);
        assert_eq!(rt.document().blocks.len(), 1);
    }

    #[test]
    fn test_voice_memo_record_and_transcribe() {
        let memo = VoiceMemo::new(Arc::new(WhisperLocalProvider));
        memo.start_recording();
        memo.tick(3.0);
        let result = memo.stop_and_transcribe();
        assert!(!result.text.is_empty());
        // 实时转写器应已插入片段
        assert_eq!(memo.realtime.segment_count(), result.segments.len());
        assert_eq!(memo.provider_name(), "whisper-local");
    }

    #[test]
    fn test_voice_memo_with_language() {
        let memo = VoiceMemo::new(Arc::new(CloudSttProvider {
            api_endpoint: "x".to_string(),
        }))
        .with_language("en");
        memo.start_recording();
        memo.tick(1.0);
        let result = memo.stop_and_transcribe();
        assert_eq!(result.language, "en");
    }

    // ---- RSS ----

    fn sample_rss_xml() -> &'static str {
        r#"<?xml version="1.0"?>
<rss version="2.0">
<channel>
  <title>Tech News Feed</title>
  <link>https://news.example.com</link>
  <description>Latest tech news</description>
  <item>
    <title>First Article</title>
    <link>https://news.example.com/1</link>
    <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
    <description>Summary of first article</description>
    <guid>guid-1</guid>
  </item>
  <item>
    <title>Second Article</title>
    <link>https://news.example.com/2</link>
    <pubDate>Tue, 02 Jan 2024 00:00:00 GMT</pubDate>
    <description>Summary of second article</description>
    <guid>guid-2</guid>
  </item>
</channel>
</rss>"#
    }

    #[test]
    fn test_rss_parser_parse_basic() {
        let feed = RssParser::parse("https://news.example.com/feed", sample_rss_xml());
        assert_eq!(feed.title, "Tech News Feed");
        assert_eq!(feed.url, "https://news.example.com/feed");
        assert_eq!(feed.items.len(), 2);
    }

    #[test]
    fn test_rss_parser_extracts_item_fields() {
        let feed = RssParser::parse("https://x.com", sample_rss_xml());
        let first = &feed.items[0];
        assert_eq!(first.title, "First Article");
        assert_eq!(first.link, "https://news.example.com/1");
        assert_eq!(first.guid, "guid-1");
        assert_eq!(first.summary, "Summary of first article");
        assert!(first.published.is_some());
    }

    #[test]
    fn test_rss_parser_pubdate_parsed() {
        let feed = RssParser::parse("https://x.com", sample_rss_xml());
        let pub_date = feed.items[0].published.unwrap();
        assert_eq!(pub_date.format("%Y-%m-%d").to_string(), "2024-01-01");
    }

    #[test]
    fn test_rss_parser_empty_feed() {
        let xml = r#"<rss><channel><title>Empty</title></channel></rss>"#;
        let feed = RssParser::parse("https://x.com", xml);
        assert_eq!(feed.title, "Empty");
        assert!(feed.items.is_empty());
    }

    #[test]
    fn test_rss_poller_detects_new_items() {
        let poller = RssPoller::new();
        let sub = RssSubscription::new("https://news.example.com/feed");
        let new = poller.poll_feed(&sub, sample_rss_xml());
        assert_eq!(new.len(), 2);
    }

    #[test]
    fn test_rss_poller_dedup_seen_items() {
        let poller = RssPoller::new();
        let sub = RssSubscription::new("https://news.example.com/feed");
        // 第一次：全部为新
        let first = poller.poll_feed(&sub, sample_rss_xml());
        assert_eq!(first.len(), 2);
        // 第二次相同 feed：无新条目
        let second = poller.poll_feed(&sub, sample_rss_xml());
        assert_eq!(second.len(), 0);
    }

    #[test]
    fn test_rss_poller_detects_only_new_items() {
        let poller = RssPoller::new();
        let sub = RssSubscription::new("https://news.example.com/feed");
        // 第一次：2 条
        poller.poll_feed(&sub, sample_rss_xml());
        // 第二次：新增 1 条
        let updated_xml = r#"<?xml version="1.0"?>
<rss version="2.0"><channel><title>Tech News Feed</title>
<item><title>First Article</title><link>https://news.example.com/1</link><guid>guid-1</guid><description>s1</description></item>
<item><title>Second Article</title><link>https://news.example.com/2</link><guid>guid-2</guid><description>s2</description></item>
<item><title>Third Article</title><link>https://news.example.com/3</link><guid>guid-3</guid><description>s3</description></item>
</channel></rss>"#;
        let new = poller.poll_feed(&sub, updated_xml);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].title, "Third Article");
    }

    #[test]
    fn test_rss_poller_15min_interval() {
        let poller = RssPoller::new();
        assert_eq!(poller.interval_seconds(), 900);
        let now = Utc::now();
        assert!(poller.should_poll(now)); // 从未轮询
        poller.mark_polled(now);
        assert!(!poller.should_poll(now)); // 刚轮询完
        let later = now + Duration::seconds(899);
        assert!(!poller.should_poll(later)); // 还差 1 秒
        let much_later = now + Duration::seconds(900);
        assert!(poller.should_poll(much_later)); // 满 15 分钟
    }

    #[test]
    fn test_rss_poller_add_subscription() {
        let poller = RssPoller::new();
        let sub = RssSubscription::new("https://feed.example.com");
        let id = sub.id.clone();
        poller.add_subscription(sub);
        assert_eq!(poller.subscription_count(), 1);
        assert_eq!(poller.subscriptions()[0].id, id);
    }

    #[test]
    fn test_rss_item_to_document() {
        let item = RssItem {
            guid: "g1".to_string(),
            title: "Test Article".to_string(),
            link: "https://example.com/1".to_string(),
            published: Some(Utc::now()),
            summary: "A summary".to_string(),
            content: "Full body text".to_string(),
        };
        let doc = RssPoller::item_to_document(&item);
        assert_eq!(doc.title, "Test Article");
        assert_eq!(
            doc.properties.get("source_link").unwrap().as_str().unwrap(),
            "https://example.com/1"
        );
        assert!(doc.properties.contains_key("published"));
        // 第一个块应为 quote（summary）
        assert!(matches!(doc.blocks[0].block_type, BlockType::Quote));
    }

    #[test]
    fn test_rss_poller_fetched_feeds_recorded() {
        let poller = RssPoller::new();
        let sub = RssSubscription::new("https://x.com");
        poller.poll_feed(&sub, sample_rss_xml());
        assert_eq!(poller.fetched_feeds().len(), 1);
        assert_eq!(poller.fetched_feeds()[0].items.len(), 2);
    }

    // ---- CaptureMatrix 顶层协调器 ----

    #[test]
    fn test_capture_matrix_routes_web_clip() {
        let cm = CaptureMatrix::new();
        let result = cm.clip_web("https://example.com", sample_html());
        assert!(matches!(result, ClipResult::Success(_)));
    }

    #[test]
    fn test_capture_matrix_routes_screenshot() {
        let cm = CaptureMatrix::new();
        let result = cm.capture_screenshot(Some(BoundingBox::default()));
        assert!(!result.image_data.is_empty());
        assert!(cm.screenshot.window.is_visible());
    }

    #[test]
    fn test_capture_matrix_routes_voice() {
        let cm = CaptureMatrix::new();
        let result = cm.record_voice(2.0);
        assert!(!result.text.is_empty());
        assert!(cm.voice.realtime.segment_count() > 0);
    }

    #[test]
    fn test_capture_matrix_routes_rss() {
        let cm = CaptureMatrix::new();
        let sub = RssSubscription::new("https://news.example.com/feed");
        let new = cm.poll_rss(&sub, sample_rss_xml());
        assert_eq!(new.len(), 2);
    }

    #[test]
    fn test_capture_matrix_default() {
        let cm = CaptureMatrix::default();
        assert_eq!(cm.rss.interval_seconds(), 900);
        assert_eq!(cm.voice.provider_name(), "whisper-local");
    }
}
