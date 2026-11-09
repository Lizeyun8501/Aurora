//! A-01: DOCX 导出 — Markdown → OOXML（.docx zip 包体，纯 Rust 零 Office 运行时）。
//!
//! # 通路披露（开工令差异）
//! 开工令写「块模型 → OOXML」；实测块模型（RawBlock）仅五类（heading/task/
//! code/quote/text），无表格/列表块——权威内容通路为笔记 Markdown 全文
//! （与 E25 桌面 MD/HTML 导出同源）。本模块输入定稿为 Markdown，覆盖面
//! 大于块模型（含 GFM 表格与列表）。
//!
//! # 覆盖（开工令最低集）
//! 标题 1-6 / 正文段 / 无序有序列表 / 任务勾选态（☐/☒）/ GFM 表格 /
//! 代码块（等宽+浅底纹）/ 引用（缩进+左框线）/ 行内粗/等宽。
//!
//! # 损失清单（v1 边界——停手边界内诚实披露）
//! - 链接 `[t](url)` 降级为 `t (url)` 纯文本（hyperlink 需 per-note rels，v1 不做）
//! - 图片 attachment:// 透传为文本链接（MD 源即如此——无附件打包，不在本卡）
//! - 列表为带符号段落（Word 真列表需 numbering 部件，工单边界「不做复杂样式保真」）

use std::io::Write;

use crate::report::ImportError;

/// OOXML w:t 文本转义。
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// run 生成：等宽/粗/斜组合。
fn run(text: &str, mono: bool, bold: bool, italic: bool) -> String {
    let mut rpr = String::new();
    if mono {
        rpr.push_str(r#"<w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:eastAsia="宋体"/>"#);
    }
    if bold {
        rpr.push_str("<w:b/>");
    }
    if italic {
        rpr.push_str("<w:i/>");
    }
    let rpr_xml = if rpr.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{rpr}</w:rPr>")
    };
    format!(
        r#"<w:r>{rpr_xml}<w:t xml:space="preserve">{}</w:t></w:r>"#,
        esc(text)
    )
}

/// 行内解析：**bold** / `code` / [t](url)→t (url)（v1 文本降级——损失清单）。
/// 有限状态机（与 E25 HTML 导出转换器同款取舍——非递归降级器）。
fn inline_runs(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let candidates = [
            rest.find("**").map(|i| (i, "**")),
            rest.find('`').map(|i| (i, "`")),
            rest.find('[').map(|i| (i, "[")),
        ];
        let Some((pos, mark)) = candidates.into_iter().flatten().min_by_key(|(i, _)| *i) else {
            out.push_str(&run(rest, false, false, false));
            break;
        };
        let (before, after) = rest.split_at(pos);
        out.push_str(&run(before, false, false, false));
        match mark {
            "**" => {
                if let Some(end) = after[2..].find("**") {
                    out.push_str(&run(&after[2..2 + end], false, true, false));
                    rest = &after[end + 4..];
                } else {
                    out.push_str(&run("**", false, false, false));
                    rest = &after[2..];
                }
            }
            "`" => {
                if let Some(end) = after[1..].find('`') {
                    out.push_str(&run(&after[1..1 + end], true, false, false));
                    rest = &after[end + 2..];
                } else {
                    out.push_str(&run("`", false, false, false));
                    rest = &after[1..];
                }
            }
            _ => {
                // [t](url) → t (url)
                if let Some(close) = after.find(']') {
                    let label = &after[1..close];
                    if after[close + 1..].starts_with('(') {
                        if let Some(paren) = after[close + 2..].find(')') {
                            let url = &after[close + 2..close + 2 + paren];
                            out.push_str(&run(&format!("{label} ({url})"), false, false, false));
                            rest = &after[close + 3 + paren..];
                            continue;
                        }
                    }
                    out.push_str(&run("[", false, false, false));
                    rest = &after[1..];
                } else {
                    out.push_str(&run("[", false, false, false));
                    rest = &after[1..];
                }
            }
        }
    }
    out
}

/// 段落（pPr 前缀由调用方注入）。
fn para(ppr: &str, runs: &str) -> String {
    let ppr_xml = if ppr.is_empty() {
        String::new()
    } else {
        format!("<w:pPr>{ppr}</w:pPr>")
    };
    format!(r#"<w:p>{ppr_xml}{runs}</w:p>"#)
}

/// 代码块段落（等宽 + 浅底纹逐行——Word 无真代码块概念）。
fn code_para(line: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:shd w:val="clear" w:fill="F5F5F5"/></w:pPr>{}</w:p>"#,
        run(line, true, false, false)
    )
}

/// GFM 表格行解析：|a|b| → cells。
fn split_table_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let core = t.strip_prefix('|').unwrap_or(t);
    let core = core.strip_suffix('|').unwrap_or(core);
    core.split('|').map(|c| c.trim().to_string()).collect()
}

/// 分隔行判定：|---|:--:|。
fn is_table_sep(line: &str) -> bool {
    let cells = split_table_row(line);
    !cells.is_empty()
        && cells.iter().all(|c| {
            let t = c.trim();
            !t.is_empty()
                && t.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')
                && t.contains('-')
        })
}

/// w:tbl 生成（首行加粗表头）。
fn table_block(header: &[String], rows: &[Vec<String>]) -> String {
    let cols = header.len();
    let mut xml = String::from(
        r#"<w:tbl><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:color="AAAAAA"/><w:left w:val="single" w:sz="4" w:color="AAAAAA"/><w:bottom w:val="single" w:sz="4" w:color="AAAAAA"/><w:right w:val="single" w:sz="4" w:color="AAAAAA"/><w:insideH w:val="single" w:sz="4" w:color="CCCCCC"/><w:insideV w:val="single" w:sz="4" w:color="CCCCCC"/></w:tblBorders></w:tblPr>"#,
    );
    xml.push_str(&format!(
        r#"<w:tblGrid>{}</w:tblGrid>"#,
        (0..cols)
            .map(|_| r#"<w:gridCol w:w="2400"/>"#)
            .collect::<String>()
    ));
    xml.push_str(r#"<w:tr><w:trPr><w:tblHeader/></w:trPr>"#);
    for cell in header {
        xml.push_str(&format!(
            r#"<w:tc><w:tcPr><w:shd w:val="clear" w:fill="EEEEEE"/></w:tcPr>{}</w:tc>"#,
            run(cell, false, true, false)
        ));
    }
    xml.push_str("</w:tr>");
    for row in rows {
        xml.push_str("<w:tr>");
        for i in 0..cols {
            let cell = row.get(i).map(String::as_str).unwrap_or("");
            xml.push_str(&format!("<w:tc>{}</w:tc>", run(cell, false, false, false)));
        }
        xml.push_str("</w:tr>");
    }
    xml.push_str("</w:tbl>");
    xml
}

/// styles.xml（docDefaults 中文字体 + Heading1-6）。
fn styles_xml() -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr>
<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="宋体" w:cs="宋体"/>
<w:sz w:val="22"/><w:szCs w:val="22"/>
</w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>"#,
    );
    for (lvl, size) in [(1u32, 32u32), (2, 26), (3, 24), (4, 22), (5, 21), (6, 20)] {
        s.push_str(&format!(
            r#"<w:style w:type="paragraph" w:styleId="Heading{lvl}"><w:name w:val="heading {lvl}"/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/></w:pPr><w:rPr><w:b/><w:sz w:val="{size}"/><w:szCs w:val="{size}"/></w:rPr></w:style>"#
        ));
    }
    s.push_str("</w:styles>");
    s
}

/// Markdown → .docx 字节。
pub fn markdown_to_docx(title: &str, md: &str) -> Result<Vec<u8>, ImportError> {
    let mut body = String::new();
    body.push_str(&format!(
        r#"<w:p><w:pPr><w:jc w:val="center"/><w:spacing w:after="240"/></w:pPr>{}</w:p>"#,
        run(title, false, true, false)
    ));

    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim_start();
        // 标题
        if let Some(rest) = t.strip_prefix('#') {
            // 剥一个 # 后剩余前缀 # 数=层级-1（旧算法把空格数当层级——# 标题 误判 lvl2 根因）
            let hashes = rest.chars().take_while(|c| *c == '#').count();
            let lvl = 1 + hashes;
            let text = rest[hashes..].trim_start();
            if (1..=6).contains(&lvl) && !text.is_empty() {
                body.push_str(&format!(
                    r#"<w:p><w:pPr><w:pStyle w:val="Heading{lvl}"/></w:pPr>{}</w:p>"#,
                    inline_runs(text)
                ));
                i += 1;
                continue;
            }
        }
        // 代码围栏
        if t.starts_with("```") {
            let mut buf: Vec<&str> = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                buf.push(lines[i]);
                i += 1;
            }
            i += 1; // 闭栏
            if buf.is_empty() {
                body.push_str(&code_para(""));
            }
            for cl in buf {
                body.push_str(&code_para(cl));
            }
            continue;
        }
        // 表格
        if t.starts_with('|') && i + 1 < lines.len() && is_table_sep(lines[i + 1]) {
            let header = split_table_row(lines[i]);
            i += 2;
            let mut rows: Vec<Vec<String>> = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                rows.push(split_table_row(lines[i]));
                i += 1;
            }
            body.push_str(&table_block(&header, &rows));
            continue;
        }
        // 任务
        if t.starts_with("- [ ] ") || t.starts_with("- [x] ") || t.starts_with("- [X] ") {
            let done = t.starts_with("- [x]") || t.starts_with("- [X]");
            let text = t[6..].to_string();
            let mark = if done { "☒ " } else { "☐ " };
            let mut runs = run(mark, false, false, false);
            runs.push_str(&inline_runs(&text));
            body.push_str(&para(r#"<w:ind w:left="360"/>"#, &runs));
            i += 1;
            continue;
        }
        // 无序列表
        if (t.starts_with("- ") || t.starts_with("* ")) && !t.starts_with("- [") {
            let text = &t[2..];
            let mut runs = run("• ", false, false, false);
            runs.push_str(&inline_runs(text));
            body.push_str(&para(r#"<w:ind w:left="360"/>"#, &runs));
            i += 1;
            continue;
        }
        // 有序列表（1. 等）
        if let Some(dot) = t.find(". ") {
            let num: String = t[..dot]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !num.is_empty() && num.len() == dot {
                let text = &t[dot + 2..];
                let mut runs = run(&format!("{num}. "), false, false, false);
                runs.push_str(&inline_runs(text));
                body.push_str(&para(r#"<w:ind w:left="360"/>"#, &runs));
                i += 1;
                continue;
            }
        }
        // 引用
        if let Some(q) = t.strip_prefix("> ") {
            body.push_str(&para(
                r#"<w:ind w:left="720"/><w:pBdr><w:left w:val="single" w:sz="12" w:color="999999" w:space="4"/></w:pBdr>"#,
                &inline_runs(q),
            ));
            i += 1;
            continue;
        }
        // 空行跳过
        if t.is_empty() {
            i += 1;
            continue;
        }
        // 正文段（连续非空行合并——与 split_markdown_blocks 段语义一致）
        let mut para_text: Vec<&str> = vec![t];
        i += 1;
        while i < lines.len() {
            let nt = lines[i].trim_start();
            if nt.is_empty()
                || nt.starts_with('#')
                || nt.starts_with("```")
                || nt.starts_with('|')
                || nt.starts_with("> ")
                || nt.starts_with("- ")
                || nt.starts_with("* ")
                || nt.starts_with("- [")
            {
                break;
            }
            para_text.push(nt);
            i += 1;
        }
        let joined = para_text.join(" ");
        body.push_str(&para("", &inline_runs(&joined)));
    }

    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#
    );

    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;

    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

    pack_zip(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/styles.xml", styles_xml().as_bytes()),
    ])
}

/// zip 包体（deflate）——pub(crate) 供 xlsx_export 复用。
pub(crate) fn pack_zip(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, ImportError> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            zip.start_file(*name, opts)
                .map_err(|e| ImportError::new(format!("zip start {name}: {e}")))?;
            zip.write_all(data)
                .map_err(|e| ImportError::new(format!("zip write {name}: {e}")))?;
        }
        zip.finish()
            .map_err(|e| ImportError::new(format!("zip finish: {e}")))?;
    }
    Ok(buf.into_inner())
}
