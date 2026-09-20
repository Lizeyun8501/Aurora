//! 良构 HTML → Markdown 转换。
//!
//! M3 范围说明：仅接受**良构 HTML**（Evernote/OneNote/Notion 等导出的
//! HTML 均满足）；浏览器随手存的非良构页面不在导入器质量目标内
//! （quick-xml 对未闭合标签直接报错 → failed 记账）。
//!
//! 映射原则与 ENML 转换器一致：文本逐字符保留（HTML 实体解码）、
//! 结构标签尽力映射、未映射标签透传 + warning。

use std::fmt::Write as _;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::enml::push_general_ref;

/// HTML → Markdown 转换结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlConversion {
    /// Markdown 正文。
    pub markdown: String,
    /// 降级/注意项。
    pub warnings: Vec<String>,
}

/// 转换良构 HTML 片段/文档为 Markdown。
///
/// `head`/`script`/`style`/`title` 等非内容元素整体跳过。
///
/// # Errors
/// HTML 非良构（quick-xml 读取失败/未闭合）。
pub fn html_to_markdown(html: &str) -> Result<HtmlConversion, String> {
    let mut conv = HtmlConversion {
        markdown: String::new(),
        warnings: Vec::new(),
    };
    // 无序列表自动序号（ol → 1. 2. 3.）
    let mut list_stack: Vec<u32> = Vec::new();
    // 表格状态：(是否已过表头, 本行单元格数)
    let mut in_table_row: Option<(bool, usize)> = None;
    let mut in_codeblock = false;
    let code_lang = String::new();
    let mut href_stack: Vec<String> = Vec::new();

    let mut reader = Reader::from_str(html);
    reader.config_mut().trim_text_start = false;
    let mut buf = Vec::new();
    let mut open_depth = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                open_depth += 1;
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"pre" => {
                        in_codeblock = true;
                        if !conv.markdown.ends_with('\n') && !conv.markdown.is_empty() {
                            conv.markdown.push('\n');
                        }
                        // pre 的 class 常含 language-xxx
                        let mut lang = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"class" {
                                let v = attr
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|err| err.to_string())?
                                    .into_owned();
                                if let Some(l) = v.strip_prefix("language-") {
                                    lang = l.to_string();
                                } else {
                                    lang = v;
                                }
                            }
                        }
                        let _ = writeln!(conv.markdown, "```{lang}");
                    }
                    b"h1" | b"h2" | b"h3" | b"h4" | b"h5" | b"h6" => {
                        let level = name[1] - b'0';
                        let _ = write!(conv.markdown, "\n{} ", "#".repeat(level as usize));
                    }
                    b"ul" => list_stack.push(0),
                    b"ol" => list_stack.push(1),
                    b"li" => {
                        let depth = list_stack.len().saturating_sub(1);
                        let indent = "  ".repeat(depth);
                        let marker = match list_stack.last() {
                            Some(&n) if n > 0 => {
                                let idx = *list_stack.last().unwrap();
                                if let Some(last) = list_stack.last_mut() {
                                    *last += 1;
                                }
                                format!("{idx}. ")
                            }
                            _ => "- ".to_string(),
                        };
                        if !conv.markdown.ends_with('\n') && !conv.markdown.is_empty() {
                            conv.markdown.push('\n');
                        }
                        let _ = write!(conv.markdown, "{indent}{marker}");
                    }
                    b"table" => {
                        in_table_row = Some((false, 0));
                        conv.markdown.push('\n');
                    }
                    b"tr" => conv.markdown.push('|'),
                    b"td" | b"th" => conv.markdown.push(' '),
                    b"blockquote" => {
                        let _ = write!(conv.markdown, "\n> ");
                    }
                    b"b" | b"strong" => conv.markdown.push_str("**"),
                    b"i" | b"em" => conv.markdown.push('*'),
                    b"a" => {
                        let mut link = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"href" {
                                link = attr
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|err| err.to_string())?
                                    .into_owned();
                            }
                        }
                        href_stack.push(link.clone());
                        let _ = write!(conv.markdown, "[");
                    }
                    b"code" if !in_codeblock => conv.markdown.push('`'),
                    // b/strong/i/em/span/div/p/font：透传内容（div/p 由 End 臂换行）
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"br" => conv.markdown.push_str("  \n"),
                    b"img" => {
                        let (mut src, mut alt) = (String::new(), String::new());
                        for attr in e.attributes().flatten() {
                            match attr.key.local_name().as_ref() {
                                b"src" => {
                                    src = attr
                                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                        .map_err(|err| err.to_string())?
                                        .into_owned();
                                }
                                b"alt" => {
                                    alt = attr
                                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                        .map_err(|err| err.to_string())?
                                        .into_owned();
                                }
                                _ => {}
                            }
                        }
                        conv.warnings
                            .push(format!("img 图片未下载，保留原链接: {src}"));
                        let alt = if alt.is_empty() {
                            "img".to_string()
                        } else {
                            alt
                        };
                        let _ = writeln!(conv.markdown, "\n![{alt}]({src})");
                    }
                    b"hr" => conv.markdown.push_str("\n---\n"),
                    _ => {}
                }
            }
            Ok(Event::CData(e)) => {
                let text = e.decode().map_err(|err| err.to_string())?;
                conv.markdown.push_str(&text);
            }
            Ok(Event::Text(e)) => {
                let text = e.decode().map_err(|err| err.to_string())?;
                if in_codeblock {
                    conv.markdown.push_str(&text);
                } else {
                    let collapsed = text.replace('\n', " ");
                    conv.markdown.push_str(&collapsed);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                push_general_ref(&mut conv.markdown, &e, &mut conv.warnings)?;
            }
            Ok(Event::End(e)) => {
                open_depth = open_depth.saturating_sub(1);
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"pre" => {
                        if !conv.markdown.ends_with('\n') {
                            conv.markdown.push('\n');
                        }
                        conv.markdown.push_str("```\n");
                        in_codeblock = false;
                        let _ = &code_lang;
                    }
                    b"h1" | b"h2" | b"h3" | b"h4" | b"h5" | b"h6" => {
                        conv.markdown.push('\n');
                    }
                    b"li" => {
                        if !conv.markdown.ends_with('\n') {
                            conv.markdown.push('\n');
                        }
                    }
                    b"ul" | b"ol" => {
                        list_stack.pop();
                        if list_stack.is_empty() {
                            conv.markdown.push('\n');
                        }
                    }
                    b"b" | b"strong" => conv.markdown.push_str("**"),
                    b"i" | b"em" => conv.markdown.push('*'),
                    b"a" => {
                        let link = href_stack.pop().unwrap_or_default();
                        let _ = write!(conv.markdown, "]({link})");
                    }
                    b"code" if !in_codeblock => conv.markdown.push('`'),
                    b"td" | b"th" => {
                        conv.markdown.push_str(" |");
                        if let Some((_, cells)) = in_table_row.as_mut() {
                            *cells += 1;
                        }
                    }
                    b"tr" => {
                        conv.markdown.push('\n');
                        if let Some((is_header, cells)) = in_table_row {
                            if !is_header {
                                let mut sep = String::new();
                                for _ in 0..cells {
                                    sep.push_str("|---");
                                }
                                sep.push('|');
                                conv.markdown.push_str(&sep);
                                conv.markdown.push('\n');
                                in_table_row = Some((true, cells));
                            }
                        }
                    }
                    b"table" => {
                        in_table_row = None;
                        conv.markdown.push('\n');
                    }
                    b"p" | b"div" => conv.markdown.push('\n'),
                    _ => {}
                }
            }
            Ok(Event::Decl(_) | Event::Comment(_) | Event::DocType(_) | Event::PI(_)) => {}
            Ok(Event::Eof) => {
                if open_depth > 0 {
                    return Err(format!("HTML 未闭合：{open_depth} 个元素缺少结束标签"));
                }
                break;
            }
            Err(err) => return Err(format!("HTML 解析失败: {err}")),
        }
        buf.clear();
    }

    while conv.markdown.contains("\n\n\n") {
        conv.markdown = conv.markdown.replace("\n\n\n", "\n\n");
    }
    Ok(conv)
}

/// 提取 `<title>…</title>` 元素文本（HTML 文档头）。
pub fn extract_title_element(html: &str) -> Option<String> {
    let mut reader = Reader::from_str(html);
    let mut buf = Vec::new();
    let mut in_title = false;
    let mut out = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"title" => in_title = true,
            Ok(Event::End(e)) if e.local_name().as_ref() == b"title" => {
                let t = out.trim().to_string();
                return if t.is_empty() { None } else { Some(t) };
            }
            Ok(Event::Text(e)) if in_title => {
                out.push_str(&e.decode().map_err(|e| e.to_string()).ok()?);
            }
            Ok(Event::Eof) => return None,
            Ok(_) => {}
            Err(_) => return None,
        }
        buf.clear();
    }
}

/// 从转换后的 Markdown 提取首个标题行文本（`# 标题`）。
pub fn extract_first_heading(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("# "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}
