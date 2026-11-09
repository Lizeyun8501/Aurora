//! A-01: XLSX 导出 — 集合（prop_defs + rows）→ 工作表（.xlsx zip 包体，纯 Rust）。
//!
//! # 列类型映射（PropertyType → 单元格类型）
//! - Text / Select / MultiSelect → inlineStr 文本
//! - Number → `n` 数值
//! - Date → 日期序列号 + numFmt `yyyy-mm-dd`（Excel 1900 纪元兼容）
//! - Checkbox → `b` 布尔（Excel 原生 TRUE/FALSE）
//! - Relation → 目标 note_id 文本（关系语义跨库不可携——损失清单）
//! - **Formula → 当前计算值 + 单元格批注「公式列：导出时计算值（表达式见列名）」**
//!   （批注 = xl/comments1.xml + vmlDrawing1.vml + sheet1 legacyDrawing 接线）
//!
//! # 通路披露（开工令差异）
//! 集合行为 note_id 引用集（rows: Vec<String>），单元格值在 props:{note_id}——
//! 导出输入由调用方（command 层）聚合为 `XlsxSheet`（defs + 行值矩阵），
//! 本模块纯函数不触 kv（write_path 铁律之外的纯导出面）。

use serde_json::Value;

use crate::docx_export::pack_zip;
use crate::report::ImportError;

/// 导出输入（command 层聚合）。
pub struct XlsxSheet {
    /// 工作表名（集合标题）。
    pub sheet_name: String,
    /// 列定义（name + 属性类型 + 公式表达式）。
    pub columns: Vec<XlsxColumn>,
    /// 行值矩阵（外层=行,内层按 columns 序;缺失=None→空单元格）。
    pub rows: Vec<Vec<Option<Value>>>,
}

/// 列描述。
pub struct XlsxColumn {
    pub name: String,
    /// PropertyType 序列化名（Text/Number/Date/Checkbox/Select/...）。
    pub prop_kind: String,
    /// Formula 表达式（非公式列为 None）。
    pub formula: Option<String>,
}

/// 列名转 Excel 列标（0→A, 26→AA）。
fn col_letter(mut idx: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, char::from(b'A' + (idx % 26) as u8));
        idx /= 26;
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    s
}

/// 日期字符串（ISO `yyyy-mm-dd` 或含时间）→ Excel 序列号（1899-12-30 起点）。
fn excel_serial(date_str: &str) -> Option<f64> {
    let d = date_str.trim().split(['T', ' ']).next()?.trim();
    let mut it = d.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let day: i64 = it.next()?.parse().ok()?;
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 71_946; // 1970-01-01 起的天数
    Some((days + 25_569) as f64) // 1900 纪元偏移
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 单元格 XML（类型映射;公式列 s=3 附加批注样式引用）。
fn cell_xml(col_idx: usize, row_idx: usize, v: &Option<Value>, kind: &str) -> String {
    let ref_cell = format!("{}{}", col_letter(col_idx), row_idx + 1);
    let Some(v) = v else {
        return format!(r#"<c r="{ref_cell}"/>"#);
    };
    match kind {
        "Number" => {
            let n = match v {
                Value::Number(n) => n.as_f64().unwrap_or(0.0),
                Value::String(s) => s.trim().parse().unwrap_or(0.0),
                _ => 0.0,
            };
            let text = if n.fract() == 0.0 {
                format!("{}", n as i64)
            } else {
                format!("{n}")
            };
            format!(r#"<c r="{ref_cell}"><v>{text}</v></c>"#)
        }
        "Date" => {
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            match excel_serial(&s) {
                Some(serial) => {
                    format!(r#"<c r="{ref_cell}" s="1"><v>{serial}</v></c>"#)
                }
                None => format!(
                    r#"<c r="{ref_cell}" t="inlineStr"><is><t xml:space="preserve">{}（无效日期）</t></is></c>"#,
                    esc(&s)
                ),
            }
        }
        "Checkbox" => {
            let b = matches!(v, Value::Bool(true))
                || matches!(v, Value::String(s) if s.eq_ignore_ascii_case("true"));
            format!(
                r#"<c r="{ref_cell}" t="b"><v>{}</v></c>"#,
                if b { 1 } else { 0 }
            )
        }
        _ => {
            let text = match v {
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                other => other.to_string(),
            };
            format!(
                r#"<c r="{ref_cell}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#,
                esc(&text)
            )
        }
    }
}

/// 集合 → .xlsx 字节。
pub fn collection_to_xlsx(sheet: &XlsxSheet) -> Result<Vec<u8>, ImportError> {
    if sheet.columns.is_empty() {
        return Err(ImportError::new(
            "集合无属性列——空 schema 不导出（禁止静默空产物）",
        ));
    }

    // 公式列（批注面）
    let formula_cols: Vec<(usize, &XlsxColumn)> = sheet
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.formula.is_some())
        .collect();
    let has_comments = !formula_cols.is_empty();

    // ---- sheet1.xml（公式列时尾接 legacyDrawing） ----
    let mut rows_xml = String::new();
    rows_xml.push_str(r#"<row r="1">"#);
    for (ci, c) in sheet.columns.iter().enumerate() {
        let name = match &c.formula {
            Some(expr) => format!("{} [公式: {}]", c.name, expr),
            None => c.name.clone(),
        };
        rows_xml.push_str(&format!(
            r#"<c r="{}1" t="inlineStr" s="2"><is><t xml:space="preserve">{}</t></is></c>"#,
            col_letter(ci),
            esc(&name)
        ));
    }
    rows_xml.push_str("</row>");
    for (ri, row) in sheet.rows.iter().enumerate() {
        rows_xml.push_str(&format!(r#"<row r="{}">"#, ri + 2));
        for (ci, v) in row.iter().enumerate() {
            let kind = sheet
                .columns
                .get(ci)
                .map(|c| c.prop_kind.as_str())
                .unwrap_or("Text");
            rows_xml.push_str(&cell_xml(ci, ri + 1, v, kind));
        }
        rows_xml.push_str("</row>");
    }
    let sheet1 = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData>{rows_xml}</sheetData>{legacy}</worksheet>"#,
        legacy = if has_comments {
            r#"<legacyDrawing r:id="rIdVml"/>"#
        } else {
            ""
        }
    );

    // ---- 批注部件 ----
    let (comments_xml, vml_xml) = if has_comments {
        let mut cx = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>Aurora 导出</author></authors><commentList>"#,
        );
        let mut shapes = String::new();
        for (ri, _row) in sheet.rows.iter().enumerate() {
            for &(ci, c) in &formula_cols {
                let expr = c.formula.as_deref().unwrap_or("");
                cx.push_str(&format!(
                    r#"<comment ref="{}{}" authorId="0"><text>公式列「{}」：此值为导出时计算结果，公式 {} 不可在 Excel 中重算。</text></comment>"#,
                    col_letter(ci),
                    ri + 2,
                    esc(&c.name),
                    esc(expr)
                ));
                shapes.push_str(&format!(
                    r##"<v:shape id="_x0000_s10{ci}" type="#_x0000_t202" style="position:absolute;margin-left:0;margin-top:0;width:108pt;height:59.25pt;z-index:1;visibility:hidden" fillcolor="#ffffe1" o:insetmode="auto"><v:fill color2="#ffffe1"/><v:shadow on="t" color="black" obscured="t"/><v:path o:connecttype="none"/><v:textbox style="mso-direction-alt:auto"><div style="text-align:left"></div></v:textbox><x:ClientData ObjectType="Note"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>1, 15, 1, 15, 2, 96, 3, 60</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{}</x:Row><x:Column>{ci}</x:Column></x:ClientData></v:shape>"##,
                    ri + 1
                ));
            }
        }
        cx.push_str("</commentList></comments>");
        let vx = format!(
            r#"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t202" coordsize="21600,21600" o:spt="202" path="m,l,21600r21600,l21600,xe"><v:stroke joinstyle="miter"/><v:path gradientshapeok="t" o:connecttype="rect"/></v:shapetype>{shapes}</xml>"#
        );
        (Some(cx), Some(vx))
    } else {
        (None, None)
    };

    // ---- workbook / rels / styles ----
    let workbook = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="SHEETNAME" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        .replace("SHEETNAME", &esc(&sheet.sheet_name));
    let wb_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    // styles: xf1=日期(numFmt164), xf2=表头粗体(fontId=1)
    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy\-mm\-dd"/></numFmts><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/></cellXfs></styleSheet>"#;

    let mut ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#
        .to_string();
    if has_comments {
        ct.push_str(r#"<Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/comments1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml"/>"#);
    }
    ct.push_str("</Types>");

    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

    let sheet_rels = if has_comments {
        Some(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdCmt" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/><Relationship Id="rIdVml" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>"#
                .to_string(),
        )
    } else {
        None
    };

    // ---- 打包 ----
    let mut entries: Vec<(&str, Vec<u8>)> = vec![
        ("[Content_Types].xml", ct.into_bytes()),
        ("_rels/.rels", root_rels.as_bytes().to_vec()),
        ("xl/workbook.xml", workbook.into_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes().to_vec()),
        ("xl/worksheets/sheet1.xml", sheet1.into_bytes()),
        ("xl/styles.xml", styles.as_bytes().to_vec()),
    ];
    if let (Some(c), Some(v), Some(r)) = (comments_xml, vml_xml, sheet_rels) {
        entries.push(("xl/comments1.xml", c.into_bytes()));
        entries.push(("xl/drawings/vmlDrawing1.vml", v.into_bytes()));
        entries.push(("xl/worksheets/_rels/sheet1.xml.rels", r.into_bytes()));
    }
    let refs: Vec<(&str, &[u8])> = entries.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    pack_zip(&refs)
}
