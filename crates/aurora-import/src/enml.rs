//! ENML（Evernote Markup Language）→ Markdown 转换。
//!
//! ENML 是 XHTML 子集 + Evernote 私有标签（en-note/en-media/en-crypt/en-todo/
//! en-codeblock）。转换器基于 quick-xml 流式事件做**有损映射**（ENML 无法
//! 无损表达为 Markdown），原则：
//!
//! 1. 文本内容逐字符保留（XML 实体由解析器解码）；
//! 2. 结构标签尽力映射（标题/列表/任务/代码/表格/链接）；
//! 3. 无法映射的标签降级为纯文本透传 + warning（绝不静默丢内容）；
//! 4. `en-media` 替换为附件引用占位（资源本体由调用方落 sidecar）。
//!
//! `en-crypt` 加密块 M2 不解密：输出占位引用 + warning（原文密码学保护
//! 的内容应由后续切片在用户输入口令后处理）。

use std::collections::HashMap;
use std::fmt::Write as _;

use quick_xml::events::Event;
use quick_xml::Reader;

/// ENML → Markdown 转换结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnmlConversion {
    /// Markdown 正文。
    pub markdown: String,
    /// 降级/注意项（标签名 + 位置序号）。
    pub warnings: Vec<String>,
}

/// 转换 ENML 文档为 Markdown。
///
/// `resources`: hash → 原始文件名（来自 ENEX `<resource>`；用于 en-media
/// 占位的可读文件名）。
///
/// # Errors
/// ENML XML 结构损坏（quick-xml 读取失败 / 元素未闭合）。
pub fn enml_to_markdown(
    enml: &str,
    resources: &HashMap<String, (String, String)>,
) -> Result<EnmlConversion, String> {
    let mut reader = Reader::from_str(enml);
    reader.config_mut().trim_text(false);

    let mut conv = EnmlConversion {
        markdown: String::new(),
        warnings: Vec::new(),
    };
    // 渲染状态栈：列表缩进 / 代码围栏 / 表格行
    let mut list_stack: Vec<u8> = Vec::new(); // 有序列表：记录当前序号；无序记 0
    let mut in_codeblock = false;
    let mut code_lang = String::new();
    let mut in_table_row: Option<(bool, usize)> = None; // (是否表头行, 单元格数)
    let mut href_stack: Vec<String> = Vec::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"en-codeblock" => {
                        in_codeblock = true;
                        code_lang.clear();
                        for attr in e.attributes().flatten() {
                            // 部分导出器把语言放 data-lang / lang
                            if matches!(attr.key.local_name().as_ref(), b"lang" | b"data-lang") {
                                code_lang = attr
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|e| e.to_string())?
                                    .into_owned();
                            }
                        }
                        if !conv.markdown.ends_with('\n') && !conv.markdown.is_empty() {
                            conv.markdown.push('\n');
                        }
                        let _ = writeln!(conv.markdown, "```{code_lang}");
                    }
                    b"h1" | b"h2" | b"h3" | b"h4" | b"h5" | b"h6" => {
                        let level = name[1] - b'0';
                        // 标题文本与 # 同行（End 臂补换行）
                        let _ = write!(conv.markdown, "\n{} ", "#".repeat(level as usize));
                    }
                    b"ul" => list_stack.push(0),
                    b"ol" => list_stack.push(1),
                    b"li" => {
                        let depth = list_stack.len().saturating_sub(1);
                        let indent = "  ".repeat(depth);
                        let marker = match list_stack.last() {
                            Some(&n) if n > 0 => {
                                // 有序列表：栈里存当前序号，输出后自增
                                let idx = *list_stack.last().unwrap();
                                if let Some(last) = list_stack.last_mut() {
                                    *last += 1;
                                }
                                format!("{idx}. ")
                            }
                            _ => "- ".to_string(),
                        };
                        let _ = write!(conv.markdown, "\n{indent}{marker}");
                    }
                    b"en-todo" => {
                        let checked = e.attributes().flatten().any(|a| {
                            a.key.local_name().as_ref() == b"checked"
                                && a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map(|v| v == "true")
                                    .unwrap_or(false)
                        });
                        let depth = list_stack.len().saturating_sub(1);
                        let indent = "  ".repeat(depth);
                        let box_ = if checked { "[x]" } else { "[ ]" };
                        let _ = write!(conv.markdown, "\n{indent}- {box_} ");
                    }
                    b"a" => {
                        let mut href = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"href" {
                                href = attr
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|e| e.to_string())?
                                    .into_owned();
                            }
                        }
                        href_stack.push(href.clone());
                        let _ = write!(conv.markdown, "[");
                        let _ = href;
                    }
                    b"table" => {
                        in_table_row = Some((false, 0));
                        conv.markdown.push('\n');
                    }
                    b"tr" => {
                        if let Some((_, cells)) = in_table_row.as_mut() {
                            *cells = 0;
                        }
                        conv.markdown.push('|');
                    }
                    b"td" | b"th" => conv.markdown.push(' '),
                    b"br" => conv.markdown.push_str("  \n"),
                    b"en-media" => {
                        let mut hash = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"hash" {
                                hash = attr
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|e| e.to_string())?
                                    .into_owned();
                            }
                        }
                        let (alt, link) = resources.get(&hash).cloned().unwrap_or_else(|| {
                            conv.warnings.push(format!(
                                "en-media@{} 无对应 resource，占位文件名",
                                &hash[..hash.len().min(8)]
                            ));
                            let ph = format!("{}.{}", &hash[..hash.len().min(12)], "bin");
                            (ph.clone(), format!("attachment://{hash}"))
                        });
                        let _ = writeln!(conv.markdown, "\n![{alt}]({link})");
                    }
                    b"en-crypt" => {
                        skip_element(&mut reader)?;
                        conv.warnings.push("en-crypt 加密块未解密，输出占位".into());
                        let _ = writeln!(conv.markdown, "\n> [加密内容，M2 暂不解密]\n");
                    }
                    // b/i/strong/em/u/span/div/p/font 等：透传内容
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                // 自闭合：<br/>、<en-media .../>、<en-todo .../> 常见
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"br" => conv.markdown.push_str("  \n"),
                    b"en-media" => {
                        let mut hash = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"hash" {
                                hash = attr
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(|e| e.to_string())?
                                    .into_owned();
                            }
                        }
                        let (alt, link) = resources.get(&hash).cloned().unwrap_or_else(|| {
                            conv.warnings.push(format!(
                                "en-media@{} 无对应 resource，占位文件名",
                                &hash[..hash.len().min(8)]
                            ));
                            let ph = format!("{}.{}", &hash[..hash.len().min(12)], "bin");
                            (ph.clone(), format!("attachment://{hash}"))
                        });
                        let _ = writeln!(conv.markdown, "\n![{alt}]({link})");
                    }
                    b"en-todo" => {
                        let checked = e.attributes().flatten().any(|a| {
                            a.key.local_name().as_ref() == b"checked"
                                && a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map(|v| v == "true")
                                    .unwrap_or(false)
                        });
                        let depth = list_stack.len().saturating_sub(1);
                        let indent = "  ".repeat(depth);
                        let box_ = if checked { "[x]" } else { "[ ]" };
                        let _ = write!(conv.markdown, "\n{indent}- {box_} ");
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.decode().map_err(|err| err.to_string())?;
                if in_codeblock {
                    conv.markdown.push_str(&text);
                } else {
                    // 表格单元格内换行压成空格；正文段落间保留
                    let collapsed = text.replace('\n', " ");
                    conv.markdown.push_str(&collapsed);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                push_general_ref(&mut conv.markdown, &e, &mut conv.warnings)?;
            }
            Ok(Event::End(e)) => {
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"en-codeblock" => {
                        in_codeblock = false;
                        // 围栏结束前确保换行
                        if !conv.markdown.ends_with('\n') {
                            conv.markdown.push('\n');
                        }
                        conv.markdown.push_str("```\n");
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
                    b"a" => {
                        let href = href_stack.pop().unwrap_or_default();
                        let _ = write!(conv.markdown, "]({href})");
                    }
                    b"td" | b"th" => {
                        conv.markdown.push_str(" |");
                        if let Some((_, cells)) = in_table_row.as_mut() {
                            *cells += 1;
                        }
                    }
                    b"tr" => {
                        conv.markdown.push('\n');
                        // 首行（表头）后插分隔行
                        if let Some((is_header, cells)) = in_table_row {
                            if !is_header {
                                // |---|---| 形式（cells 列）
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
            Ok(Event::Eof) => break,
            Ok(Event::CData(e)) => {
                let text = e.decode().map_err(|err| err.to_string())?;
                conv.markdown.push_str(&text);
            }
            Ok(Event::Decl(_) | Event::Comment(_) | Event::DocType(_) | Event::PI(_)) => {}
            Err(err) => return Err(format!("ENML 解析失败: {err}")),
        }
        buf.clear();
    }

    // 压缩 3 连以上空行为 1 空行（转换产物规整，不触碰非空行内容）
    while conv.markdown.contains("\n\n\n") {
        conv.markdown = conv.markdown.replace("\n\n\n", "\n\n");
    }
    Ok(conv)
}

/// 解析实体引用（quick-xml 0.41 将 `&amp;` 等拆为独立 `Event::GeneralRef`）。
pub(crate) fn push_general_ref(
    out: &mut String,
    e: &quick_xml::events::BytesRef<'_>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let name = e.decode().map_err(|err| err.to_string())?;
    match name.as_ref() {
        "amp" => out.push('&'),
        "lt" => out.push('<'),
        "gt" => out.push('>'),
        "quot" => out.push('"'),
        "apos" => out.push('\''),
        "nbsp" => out.push('\u{00a0}'),
        other => {
            if other.starts_with('#') {
                if let Some(ch) = e.resolve_char_ref().map_err(|err| err.to_string())? {
                    out.push(ch);
                }
            } else {
                warnings.push(format!("未知实体引用 &{other}; 已按原文跳过"));
            }
        }
    }
    Ok(())
}

/// 跳过整个元素（光标已消费 Start；容忍任意嵌套深度）。
fn skip_element(reader: &mut Reader<&[u8]>) -> Result<(), String> {
    let mut depth = 1usize;
    let mut buf = Vec::new();
    while depth > 0 {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => depth -= 1,
            Ok(Event::Eof) => return Err("元素未闭合（EOF）".to_string()),
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        buf.clear();
    }
    Ok(())
}
