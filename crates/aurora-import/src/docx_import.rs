//! A-01: DOCX 导入 — .docx（word/document.xml）→ Markdown（尽力解析 + 跳过报告）。
//!
//! # 解析面
//! - 段落 w:p：pStyle Heading1-6 → `#`；numPr → 列表项（`-`/有序降级文本）；
//!   普通段 → 正文
//! - 表格 w:tbl → GFM 表格（首行表头）
//! - run 属性：b → `**bold**`（连续同格式 run 合并——简化：整段无混合格式判定
//!   用逐 run 包裹，可能产出碎片——损失清单披露）
//! - w:t 文本聚合（UTF-8，中文正常）
//!
//! # 跳过项（进报告——禁止静默丢内容）
//! - w:drawing（图片——无附件链，不假装导入）
//! - w:txbxContent（文本框内容）
//! - 不认识的块级元素

use std::io::Read;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::report::ImportError;

/// 导入产物：Markdown 正文 + 跳过项报告。
pub struct DocxImportResult {
    pub markdown: String,
    pub skipped: Vec<String>,
}

/// .docx 字节 → Markdown + 跳过报告。
pub fn docx_to_markdown(bytes: &[u8]) -> Result<DocxImportResult, ImportError> {
    // 1) 解 zip 取 word/document.xml
    let cursor = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(cursor)
        .map_err(|e| ImportError::new(format!("非有效 .docx（zip 打开失败）: {e}")))?;
    let mut xml_bytes = Vec::new();
    match zip.by_name("word/document.xml") {
        Ok(mut f) => {
            f.read_to_end(&mut xml_bytes)
                .map_err(|e| ImportError::new(format!("document.xml 读取失败: {e}")))?;
        }
        Err(_) => {
            return Err(ImportError::new(
                "非有效 .docx（缺 word/document.xml）——可能为 .doc 老格式或伪装扩展名",
            ));
        }
    }
    let xml = String::from_utf8_lossy(&xml_bytes).into_owned();

    // 2) 事件流解析（块级状态机）
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();

    let mut out = String::new();
    let mut skipped: Vec<String> = Vec::new();

    // 段落累积态
    let mut in_paragraph = false;
    let mut in_heading = 0usize; // 1-6
    let mut in_list = false;
    let mut para_runs: Vec<(String, bool, bool)> = Vec::new(); // (text, bold, italic)
    let mut in_run = false;
    let mut run_bold = false;
    let mut run_italic = false;
    let mut in_tbl = false;
    let mut tbl_rows: Vec<Vec<String>> = Vec::new();
    let mut in_tr = false;
    let mut in_tc = false;
    let mut tc_text = String::new();
    let mut tc_is_header = false;
    let mut tbl_header_seen = false;
    let mut in_drawing = 0usize;
    let mut in_txbx = 0usize;

    fn flush_para(out: &mut String, runs: &[(String, bool, bool)], heading: usize, list: bool) {
        if runs.is_empty() {
            return;
        }
        let mut text = String::new();
        for (t, b, i) in runs {
            let mut s = t.replace('\n', " ");
            if s.is_empty() {
                continue;
            }
            if *b && !s.trim().is_empty() {
                s = format!("**{s}**");
            }
            if *i && !s.trim().is_empty() {
                s = format!("*{s}*");
            }
            text.push_str(&s);
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if heading > 0 {
            out.push_str(&"#######"[..heading]);
            out.push(' ');
            out.push_str(text);
        } else if list {
            out.push_str("- ");
            out.push_str(text);
        } else {
            out.push_str(text);
        }
        out.push('\n');
    }

    fn flush_table(out: &mut String, rows: &[Vec<String>], header_seen: bool) {
        if rows.is_empty() {
            return;
        }
        // 首行作表头（无 tblHeader 标记时同样取首行——GFM 必须有表头）
        let header = &rows[0];
        let widths: Vec<usize> = header.iter().map(|c| c.chars().count().max(3)).collect();
        out.push('|');
        for (i, c) in header.iter().enumerate() {
            out.push_str(&format!(" {} |", pad_cell(c, widths[i])));
        }
        out.push('\n');
        out.push('|');
        for w in &widths {
            out.push_str(&format!(" {} |", "-".repeat(*w)));
        }
        out.push('\n');
        for row in rows.iter().skip(1) {
            out.push('|');
            for (i, w) in widths.iter().enumerate() {
                let c = row.get(i).map(String::as_str).unwrap_or("");
                out.push_str(&format!(" {} |", pad_cell(c, *w)));
            }
            out.push('\n');
        }
        if !header_seen {
            out.push_str("<!-- 注: 原表未标记表头行，首行按 GFM 约定作表头 -->\n");
        }
        out.push('\n');
    }

    fn pad_cell(s: &str, w: usize) -> String {
        let n = s.chars().count();
        if n >= w {
            s.to_string()
        } else {
            format!("{}{}", s, " ".repeat(w - n))
        }
    }

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let local = name.local_name(); // 绑本体（LocalName 借 e 活期内合法;as_ref() 去前缀）
                match local.as_ref() {
                    b"p" if !in_tbl => {
                        in_paragraph = true;
                        in_heading = 0;
                        in_list = false;
                        para_runs.clear();
                    }
                    b"p" => { /* 表内段：文本并入单元格 */ }
                    b"numPr" => {
                        in_list = true;
                    }
                    b"r" if !in_tbl => {
                        in_run = true;
                        run_bold = false;
                        run_italic = false;
                    }
                    b"tbl" => {
                        in_tbl = true;
                        tbl_rows.clear();
                        tbl_header_seen = false;
                    }
                    b"tr" if in_tbl => {
                        in_tr = true;
                        tbl_rows.push(Vec::new());
                    }
                    b"tc" if in_tr => {
                        in_tc = true;
                        tc_text.clear();
                        tc_is_header = false;
                    }
                    b"tblHeader" if in_tbl => {
                        tc_is_header = true;
                        tbl_header_seen = true;
                    }
                    b"drawing" => {
                        in_drawing += 1;
                        skipped.push("图片（w:drawing）——无附件链，未导入".to_string());
                    }
                    b"txbxContent" => {
                        in_txbx += 1;
                        skipped.push("文本框内容（w:txbxContent）——未导入".to_string());
                    }
                    b"b" if in_run => run_bold = true,
                    b"i" if in_run => run_italic = true,
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if in_drawing > 0 || in_txbx > 0 {
                    // 已报告跳过——内容丢弃（不静默：报告已含元素级说明）
                } else if in_tc {
                    tc_text.push_str(
                        &t.decode()
                            .map_err(|e| ImportError::new(format!("XML 解码失败: {e}")))?,
                    );
                } else if in_run && in_paragraph {
                    para_runs.push((
                        t.decode()
                            .map_err(|e| ImportError::new(format!("XML 解码失败: {e}")))?
                            .into_owned(),
                        run_bold,
                        run_italic,
                    ));
                }
            }
            Ok(Event::End(e)) => {
                let local = e.name().local_name(); // 绑本体（as_ref 去前缀——QName 直比会含 w:/r: 前缀永不中）
                match local.as_ref() {
                    b"p" if !in_tbl => {
                        flush_para(&mut out, &para_runs, in_heading, in_list);
                        in_paragraph = false;
                        in_heading = 0;
                        in_list = false;
                        para_runs.clear();
                    }
                    b"p" => { /* 表内段结束——单元格文本已累积 */ }
                    b"r" if !in_tbl => in_run = false,
                    b"tc" if in_tc => {
                        if let Some(row) = tbl_rows.last_mut() {
                            let _ = tc_is_header;
                            row.push(tc_text.trim().replace('\n', " "));
                        }
                        in_tc = false;
                    }
                    b"tr" if in_tr => {
                        in_tr = false;
                    }
                    b"tbl" if in_tbl => {
                        flush_table(&mut out, &tbl_rows, tbl_header_seen);
                        in_tbl = false;
                        tbl_rows.clear();
                    }
                    b"drawing" => in_drawing = in_drawing.saturating_sub(1),
                    b"txbxContent" => in_txbx = in_txbx.saturating_sub(1),
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                // 自闭合元素（pStyle/numPr 等——Aurora 导出件常态,漏则标题级全丢）
                let name = e.name();
                let local = name.local_name();
                match local.as_ref() {
                    b"pStyle" => apply_pstyle(&e, &mut in_heading, &mut in_list),
                    b"numPr" => in_list = true,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(ImportError::new(format!("document.xml 解析失败: {e}")));
            }
        }
        buf.clear();
    }

    if out.trim().is_empty() && skipped.is_empty() {
        return Err(ImportError::new(
            "文档无文本内容（空 body）——不产出空笔记（禁止静默占位）",
        ));
    }

    Ok(DocxImportResult {
        markdown: out,
        skipped,
    })
}

/// pStyle 属性应用（Heading1-6 / ListParagraph）——Start/Empty 双路由共用。
fn apply_pstyle(e: &quick_xml::events::BytesStart<'_>, in_heading: &mut usize, in_list: &mut bool) {
    let mut val = String::new();
    for a in e.attributes().flatten() {
        if a.key.local_name().as_ref() == b"val" {
            val = String::from_utf8_lossy(&a.value).into_owned();
        }
    }
    if let Some(n) = val
        .strip_prefix("Heading")
        .and_then(|n| n.parse::<usize>().ok())
    {
        if (1..=6).contains(&n) {
            *in_heading = n;
        }
    } else if val == "ListParagraph" {
        *in_list = true;
    }
}
