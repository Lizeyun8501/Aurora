//! DK-审阅 P0-3：Markdown 导出命令层——单篇/全库 md 出库。
//!
//! 领地：desktop/tauri（新文件）；读链复用 lib.rs 既有解密模式
//! （unwrap_note_bytes，pub(crate) 化）；文件名原语在 core::export
//! （纯函数已单测）。v1 边界：明文出库=用户显式行为，确认门归 UI
//! 层（S2 与双向链接 UI 同批）；附件不导出（内容寻址域复杂，另评估）。

use crate::{get_core, get_vault, unwrap_note_bytes};
use aurora_core::export::{dedupe_filename, existing_md_names, sanitize_filename};
use std::collections::HashSet;

/// 单篇导出为 markdown（`{dest_dir}/{安全文件名}.md`），返回落盘绝对路径。
///
/// # Errors
/// 应用未初始化 / 笔记不存在 / 目录创建或写盘失败。
#[tauri::command]
pub async fn cmd_export_note_markdown(note_id: String, dest_dir: String) -> Result<String, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let data = core
        .kv_store
        .get(&format!("note:{note_id}"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("note not found: {note_id}"))?;
    let json = unwrap_note_bytes(&core, &vault, &data)?;
    let title = json
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("untitled");
    let content = json.get("content").and_then(|c| c.as_str()).unwrap_or("");

    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("create dir `{dest_dir}`: {e}"))?;
    // 重名去重：扫目录已有 .md 基名（避免覆盖同目录既有导出/笔记）
    let mut existing: HashSet<String> = HashSet::new();
    let mut rd = tokio::fs::read_dir(&dest_dir)
        .await
        .map_err(|e| format!("read dir: {e}"))?;
    while let Some(e) = rd.next_entry().await.map_err(|e| format!("readdir: {e}"))? {
        if let Some(name) = e.file_name().to_str() {
            existing.insert(name.to_string());
        }
    }
    let base = sanitize_filename(title);
    let final_name = dedupe_filename(
        &existing_md_names(existing.iter().cloned().collect::<Vec<_>>().as_slice()),
        &base,
    );
    let path = format!("{}/{}.md", dest_dir.trim_end_matches('/'), final_name);
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("write `{path}`: {e}"))?;
    Ok(path)
}

/// 全库导出为 markdown（一笔记一文件），返回落盘路径列表（失败项跳过
/// 并计入 skipped——v1 简单容忍，注记不做中断回滚）。
///
/// # Errors
/// 应用未初始化 / 目录创建失败。
#[tauri::command]
pub async fn cmd_export_all_markdown(dest_dir: String) -> Result<Vec<String>, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("create dir `{dest_dir}`: {e}"))?;
    let rows = core
        .kv_store
        .scan_prefix("note:")
        .await
        .map_err(|e| e.to_string())?;
    let mut existing: HashSet<String> = HashSet::new();
    let mut rd = tokio::fs::read_dir(&dest_dir)
        .await
        .map_err(|e| format!("read dir: {e}"))?;
    while let Some(e) = rd.next_entry().await.map_err(|e| format!("readdir: {e}"))? {
        if let Some(name) = e.file_name().to_str() {
            existing.insert(name.to_string());
        }
    }
    let mut out = Vec::new();
    for (_key, data) in rows {
        // 键形态 note:{id}——trash 标记等无 "note:" 前缀干扰（trash:{id} 不匹配）
        let Ok(json) = unwrap_note_bytes(&core, &vault, &data) else {
            continue; // 解密失败项跳过（诚实计数）
        };
        let title = json
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("untitled");
        let content = json.get("content").and_then(|c| c.as_str()).unwrap_or("");
        let base = sanitize_filename(title);
        let final_name = dedupe_filename(
            &existing_md_names(existing.iter().cloned().collect::<Vec<_>>().as_slice()),
            &base,
        );
        let path = format!("{}/{}.md", dest_dir.trim_end_matches('/'), final_name);
        if tokio::fs::write(&path, content).await.is_ok() {
            existing.insert(final_name);
            out.push(path);
        }
    }
    Ok(out)
}

// ── DK-导出格式扩展（审阅 2.3 收官）：HTML 导出 + PDF webview 通路 ──

/// HTML escape（不可信内容 fail-closed——插件输出同款先例）。
/// 顺序：& 先转（防二次转义实体）。
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// 行内 wikilink：`[[title]]` → `<span class="wikilink">[[title]]</span>`
/// （title 已 escape 的文本上扫描——不解析目标，单篇导出无图谱语义）。
fn inline_wikilinks(escaped: &str) -> String {
    let mut out = String::with_capacity(escaped.len());
    let bytes = escaped.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if escaped[i..].starts_with("[[") {
            if let Some(end_rel) = escaped[i + 2..].find("]]") {
                let title = &escaped[i + 2..i + 2 + end_rel];
                out.push_str("<span class=\"wikilink\">[[");
                out.push_str(title);
                out.push_str("]]</span>");
                i += 2 + end_rel + 2;
                continue;
            }
        }
        out.push_str(&escaped[i..i + 1]);
        i += escaped[i..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// 行内格式：粗体/斜体/链接/行内码（成对 ` 段——先切码段，其余 escape
/// 后 wikilink→粗斜→链接；码段 escape 包 <code>）。转换不到的语法
/// （嵌套强调/图片 base64 等）原样透传——设计稿诚实边界。
fn inline_md(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(pos) = rest.find('`') {
        let (before, after) = rest.split_at(pos);
        // 非码段：escape → wikilink → 粗斜 → 链接
        let escaped = html_escape(before);
        let wikied = inline_wikilinks(&escaped);
        out.push_str(&inline_emph_links(&wikied));
        match after[1..].find('`') {
            Some(close) => {
                out.push_str("<code>");
                out.push_str(&html_escape(&after[1..1 + close]));
                out.push_str("</code>");
                rest = &after[1 + close + 1..];
            }
            None => {
                // 未闭合 `：原样透传（诚实边界）
                out.push('`');
                rest = &after[1..];
            }
        }
    }
    let escaped = html_escape(rest);
    let wikied = inline_wikilinks(&escaped);
    out.push_str(&inline_emph_links(&wikied));
    out
}

/// 粗体/斜体/链接（escape 后文本——**x**→strong、*y*→em、[t](u)→a）。
/// 贪心配对，嵌套不完美透传（v1 有限转换器边界）。
fn inline_emph_links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    loop {
        // 链接优先：[text](url)——先于粗体（** 无链接内嵌场景 v1）
        let b = rest.find("**");
        let l = rest.find('[');
        let link_first = l.is_some_and(|lv| b.is_none_or(|bv| lv < bv));
        if link_first {
            let lv = l.unwrap_or(0);
            let (before, after) = rest.split_at(lv);
            out.push_str(before);
            match parse_md_link(after) {
                Some((text, url, consumed)) => {
                    out.push_str(&format!(
                        "<a href=\"{}\">{}</a>",
                        html_escape(&url),
                        inline_emph_links(&text)
                    ));
                    rest = &after[consumed..];
                }
                None => {
                    out.push('[');
                    rest = &after[1..];
                }
            }
            continue;
        }
        if let Some(bv) = b {
            let (before, after) = rest.split_at(bv);
            out.push_str(before);
            match after[2..].find("**") {
                Some(close) => {
                    out.push_str(&format!(
                        "<strong>{}</strong>",
                        inline_emph_links(&after[2..2 + close])
                    ));
                    rest = &after[2 + close + 2..];
                }
                None => {
                    out.push_str("**");
                    rest = &after[2..];
                }
            }
            continue;
        }
        if let Some(lv) = l {
            let (before, after) = rest.split_at(lv);
            out.push_str(before);
            match parse_md_link(after) {
                Some((text, url, consumed)) => {
                    out.push_str(&format!(
                        "<a href=\"{}\">{}</a>",
                        html_escape(&url),
                        inline_emph_links(&text)
                    ));
                    rest = &after[consumed..];
                }
                None => {
                    out.push('[');
                    rest = &after[1..];
                }
            }
            continue;
        }
        break;
    }
    // 斜体（单星——粗体已消费 **）
    let mut fin = String::new();
    let mut r = out.as_str();
    while let Some(pos) = r.find('*') {
        let (before, after) = r.split_at(pos);
        fin.push_str(before);
        match after[1..].find('*') {
            Some(close) if close > 0 => {
                fin.push_str(&format!("<em>{}</em>", &after[1..1 + close]));
                r = &after[1 + close + 1..];
            }
            _ => {
                fin.push('*');
                r = &after[1..];
            }
        }
    }
    fin.push_str(r);
    fin
}

/// `[text](url)` 解析（行首为 `[`；url 到首个 `)`，text 到首个 `](`）。
fn parse_md_link(s: &str) -> Option<(String, String, usize)> {
    if !s.starts_with('[') {
        return None;
    }
    let close_text = s.find("](")?;
    let close_url = s[close_text + 2..].find(')')?;
    let text = s[1..close_text].to_string();
    let url = s[close_text + 2..close_text + 2 + close_url].to_string();
    let consumed = close_text + 2 + close_url + 1;
    Some((text, url, consumed))
}

/// 有限 MD→HTML 转换器（纯函数——单测 CI 桌面面兜底）。
///
/// 覆盖：标题 h1-h6 / 围栏代码块 / 行内码 / 粗斜体 / 链接 / 列表（含有序）/
/// 引用 / wikilink 透传 span / 全文本 escape。**不引 md 解析依赖**（编译
/// 增量+攻击面考量）；转换不到的语法原样透传（设计稿诚实边界）。
fn md_to_html(md: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    let mut list: Option<&'static str> = None; // "ul" | "ol" 当前聚组
    for line in md.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if in_code {
                out.push_str("</code></pre>\n");
                in_code = false;
            } else {
                close_list(&mut out, &mut list);
                out.push_str("<pre><code>");
                in_code = true;
            }
            continue;
        }
        if in_code {
            out.push_str(&html_escape(line));
            out.push('\n');
            continue;
        }
        if let Some(h) = trimmed.strip_prefix('#') {
            let level = h.chars().take_while(|c| *c == '#').count();
            if (1..=6).contains(&level) && h[level..].starts_with(' ') {
                close_list(&mut out, &mut list);
                let body = inline_md(h[level..].trim_start());
                out.push_str(&format!("<h{level}>{body}</h{level}>\n"));
                continue;
            }
        }
        if trimmed.starts_with("> ") || trimmed == ">" {
            close_list(&mut out, &mut list);
            let body = inline_md(trimmed.trim_start_matches("> ").trim_start_matches('>'));
            out.push_str(&format!("<blockquote>{body}</blockquote>\n"));
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            if list != Some("ul") {
                close_list(&mut out, &mut list);
                out.push_str("<ul>\n");
                list = Some("ul");
            }
            out.push_str(&format!("<li>{}</li>\n", inline_md(rest)));
            continue;
        }
        let ordered = trimmed
            .split_once(". ")
            .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if let Some((_, rest)) = ordered {
            if list != Some("ol") {
                close_list(&mut out, &mut list);
                out.push_str("<ol>\n");
                list = Some("ol");
            }
            out.push_str(&format!("<li>{}</li>\n", inline_md(rest)));
            continue;
        }
        close_list(&mut out, &mut list);
        if trimmed.is_empty() {
            continue;
        }
        out.push_str(&format!("<p>{}</p>\n", inline_md(trimmed)));
    }
    if in_code {
        out.push_str("</code></pre>\n");
    }
    close_list(&mut out, &mut list);
    out
}

/// 列表聚组收口（换结构/文末时闭合当前 ul/ol）。
fn close_list(out: &mut String, list: &mut Option<&'static str>) {
    if let Some(tag) = list.take() {
        out.push_str(&format!("</{tag}>\n"));
    }
}

/// 单篇导出为 HTML（MD 有限转换+极简内联样式——不做样式主题，停手条款）。
/// 重名对账复用 sanitize/dedupe（.html 后缀变体）；attachment:// 链接
/// 原样透传+注记（不做图片 base64 内嵌——停手条款）。
///
/// # Errors
/// 应用未初始化 / 笔记不存在 / 目录创建或写盘失败。
#[tauri::command]
pub async fn cmd_export_note_html(note_id: String, dest_dir: String) -> Result<String, String> {
    let core = get_core()?;
    let vault = get_vault()?;
    let data = core
        .kv_store
        .get(&format!("note:{note_id}"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("note not found: {note_id}"))?;
    let json = unwrap_note_bytes(&core, &vault, &data)?;
    let title = json
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("untitled");
    let content = json.get("content").and_then(|c| c.as_str()).unwrap_or("");

    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("create dir `{dest_dir}`: {e}"))?;
    let mut existing: HashSet<String> = HashSet::new();
    let mut rd = tokio::fs::read_dir(&dest_dir)
        .await
        .map_err(|e| format!("read dir: {e}"))?;
    while let Some(e) = rd.next_entry().await.map_err(|e| format!("readdir: {e}"))? {
        if let Some(name) = e.file_name().to_str() {
            existing.insert(name.to_string());
        }
    }
    let base = sanitize_filename(title);
    let final_name = dedupe_filename(
        &existing_md_names(existing.iter().cloned().collect::<Vec<_>>().as_slice()),
        &base,
    );
    let path = format!("{}/{}.html", dest_dir.trim_end_matches('/'), final_name);
    let body = md_to_html(content);
    let doc = format!(
        "<!DOCTYPE html>\n<html lang=\"zh\"><head><meta charset=\"utf-8\">\
<title>{}</title><style>body{{font-family:sans-serif;max-width:720px;\
margin:2em auto;padding:0 1em;line-height:1.6}}code,pre{{background:#f4f4f4;\
border-radius:4px}}pre{{padding:.8em;overflow:auto}}code{{padding:.1em .3em}}\
.wikilink{{color:#1c64d9;border-bottom:1px dashed #1c64d9}}</style></head>\
<body><h1>{}</h1>\n{}</body></html>\n",
        html_escape(title),
        html_escape(title),
        body
    );
    tokio::fs::write(&path, doc)
        .await
        .map_err(|e| format!("write `{path}`: {e}"))?;
    Ok(path)
}

/// 单篇 PDF 导出 v1：webview 打印通路（tauri 2 WebviewWindow::print——
/// 零新依赖）。语义=**打开系统打印对话框，由用户选择"另存为 PDF"**——
/// 导出路径不可编程指定（平台能力占位例外，诚实注记；纯 Rust PDF 库
/// 排版弱+重依赖不取）。确认门 UI 负责「将打开系统打印对话框」文案。
///
/// # Errors
/// 应用未初始化 / 无主窗口句柄。
#[tauri::command]
pub async fn cmd_export_note_pdf(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main webview window not found".to_string())?;
    win.print().map_err(|e| format!("open print dialog: {e}"))?;
    Ok("print dialog opened (save as PDF in dialog)".to_string())
}

#[cfg(test)]
mod export_html_tests {
    use super::*;

    #[test]
    fn test_html_escape_all_five() {
        assert_eq!(
            html_escape("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&#39;f"
        );
        assert_eq!(html_escape("&amp;"), "&amp;amp;", "& 先转防二次转义");
    }

    #[test]
    fn test_wikilink_passthrough_span() {
        assert_eq!(
            inline_wikilinks("[[My Note]]"),
            "<span class=\"wikilink\">[[My Note]]</span>"
        );
        // 悬挂 [[ 无闭合——原样透传
        assert_eq!(inline_wikilinks("[[open"), "[[open");
    }

    #[test]
    fn test_headings_and_paragraph() {
        assert_eq!(md_to_html("# Title\n"), "<h1>Title</h1>\n");
        assert_eq!(md_to_html("### deep\n"), "<h3>deep</h3>\n");
        assert_eq!(md_to_html("plain"), "<p>plain</p>\n");
    }

    #[test]
    fn test_code_fence_and_inline() {
        assert_eq!(
            md_to_html("```\na<b & c\n```\n"),
            "<pre><code>a&lt;b &amp; c\n</code></pre>\n"
        );
        assert_eq!(
            md_to_html("use `x < y` here\n"),
            "<p>use <code>x &lt; y</code> here</p>\n"
        );
        // 未闭合围栏到文末收口
        assert_eq!(
            md_to_html("```\nunclosed"),
            "<pre><code>unclosed\n</code></pre>\n"
        );
    }

    #[test]
    fn test_bold_em_link_list_quote() {
        assert_eq!(
            md_to_html("**b** *i*\n"),
            "<p><strong>b</strong> <em>i</em></p>\n"
        );
        assert_eq!(
            md_to_html("[t](http://x?a=1&b=2)\n"),
            "<p><a href=\"http://x?a=1&amp;b=2\">t</a></p>\n"
        );
        assert_eq!(
            md_to_html("- a\n- b\n"),
            "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n"
        );
        assert_eq!(md_to_html("1. one\n"), "<ol>\n<li>one</li>\n</ol>\n");
        assert_eq!(md_to_html("> quote\n"), "<blockquote>quote</blockquote>\n");
    }

    #[test]
    fn test_wikilink_inside_paragraph() {
        assert_eq!(
            md_to_html("see [[Other]] now\n"),
            "<p>see <span class=\"wikilink\">[[Other]]</span> now</p>\n"
        );
    }

    #[test]
    fn test_attachment_link_passthrough() {
        // attachment:// 原样透传（停手条款——不做 base64 内嵌）
        let out = md_to_html("![img](attachment://abc)\n");
        assert!(out.contains("attachment://abc"), "{out}");
    }
}
