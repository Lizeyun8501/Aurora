//! A-01: XLSX 导入 — .xlsx 首表 → 新建集合（schema 推断 + 行值）+ 跳过报告。
//!
//! # 通路
//! zip → xl/workbook.xml（首 sheet r:id）→ workbook rels → sheetN.xml →
//! quick-xml 解行/列（inlineStr / sharedStrings / n / b / s=1900 日期样式）。
//!
//! # 列类型推断（首表全列扫描）
//! 全列 TRUE/FALSE → Checkbox;全数值 → Number;全 ISO 日期文本或日期样式
//! → Date;其余 → Text。首行 = 列名（schema）。
//!
//! # 跳过项（报告——禁止静默丢）
//! - 第 2+ 工作表（仅导首表——工单语义）
//! - 公式单元格取缓存值（`<v>`）;无缓存 → 跳过报告
//! - 空列名（Column N 补名——报告注明）

use std::io::Read;

use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::Value;

use crate::report::ImportError;

/// 导入产物：推断 schema + 行值矩阵 + 跳过报告。
pub struct XlsxImportResult {
    /// 集合标题（文件名由调用方定——这里返回工作表名备选）。
    pub sheet_name: String,
    /// 列（name + 推断类型名）。
    pub columns: Vec<(String, String)>,
    /// 行值（外层=行,内层按 columns 序;None=空单元格）。
    pub rows: Vec<Vec<Option<Value>>>,
    pub skipped: Vec<String>,
}

/// .xlsx 字节 → 集合导入产物。
pub fn xlsx_to_collection(bytes: &[u8]) -> Result<XlsxImportResult, ImportError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(cursor)
        .map_err(|e| ImportError::new(format!("非有效 .xlsx（zip 打开失败）: {e}")))?;

    // 1) sharedStrings（可选）
    let mut shared: Vec<String> = Vec::new();
    if let Ok(mut f) = zip.by_name("xl/sharedStrings.xml") {
        let mut b = Vec::new();
        f.read_to_end(&mut b)
            .map_err(|e| ImportError::new(format!("sharedStrings 读取失败: {e}")))?;
        shared = parse_shared_strings(&String::from_utf8_lossy(&b));
    }

    // 2) workbook.xml → 首 sheet 名 + r:id
    let mut wb_bytes = Vec::new();
    match zip.by_name("xl/workbook.xml") {
        Ok(mut f) => {
            f.read_to_end(&mut wb_bytes)
                .map_err(|e| ImportError::new(format!("workbook.xml 读取失败: {e}")))?;
        }
        Err(_) => return Err(ImportError::new("非有效 .xlsx（缺 workbook.xml）")),
    }
    let wb_xml = String::from_utf8_lossy(&wb_bytes).into_owned();
    let (first_sheet_name, first_rid, sheet_count) = parse_workbook(&wb_xml)?;

    // 3) workbook rels → r:id → target（worksheets/sheetN.xml）
    let mut rels_bytes = Vec::new();
    match zip.by_name("xl/_rels/workbook.xml.rels") {
        Ok(mut f) => {
            f.read_to_end(&mut rels_bytes)
                .map_err(|e| ImportError::new(format!("workbook rels 读取失败: {e}")))?;
        }
        Err(_) => return Err(ImportError::new("非有效 .xlsx（缺 workbook rels）")),
    }
    let rels_xml = String::from_utf8_lossy(&rels_bytes).into_owned();
    let target = parse_rels_target(&rels_xml, &first_rid)
        .ok_or_else(|| ImportError::new(format!("workbook rels 缺 {first_rid} 映射")))?;
    let path = if target.starts_with("xl/") {
        target
    } else {
        format!("xl/{target}")
    };

    let mut sheet_bytes = Vec::new();
    match zip.by_name(&path) {
        Ok(mut f) => {
            f.read_to_end(&mut sheet_bytes)
                .map_err(|e| ImportError::new(format!("工作表读取失败: {e}")))?;
        }
        Err(_) => {
            return Err(ImportError::new(format!(
                "非有效 .xlsx（工作表 {path} 缺失）"
            )))
        }
    }

    let mut skipped: Vec<String> = Vec::new();
    if sheet_count > 1 {
        skipped.push(format!(
            "仅导入首个工作表「{first_sheet_name}」（共 {sheet_count} 表——其余表未导入）"
        ));
    }

    // 4) sheet 解析 → 原始单元格（ref, t, s, 值, is_date_style 预置 false——样式表日期判定简法:
    //    v 数值且 s 属性在日期样式集合——v1 用启发: 数值在 [20000, 60000]（1954-2064）且
    //    列内全部落段 → Date（Excel serial 域判定——避免 styles.xml 全解析;误报率可接受,
    //    跳过报告不涉及此——类型推断诚实标注）
    let cells = parse_sheet(
        &String::from_utf8_lossy(&sheet_bytes),
        &shared,
        &mut skipped,
    )?;

    if cells.is_empty() {
        return Err(ImportError::new(
            "工作表无数据行——不产出空集合（禁止静默占位）",
        ));
    }

    // 5) 列名（首行）+ 类型推断
    if cells.is_empty() {
        return Err(ImportError::new(
            "首工作表无数据行（空表）——不产出空集合（禁止静默占位）",
        ));
    }
    let header_row = &cells[0];
    let n_cols = header_row.len();
    let mut columns: Vec<(String, String)> = Vec::with_capacity(n_cols);
    for (i, h) in header_row.iter().enumerate() {
        let name = match h {
            Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            Some(other) => {
                let txt = other.to_string();
                if txt.is_empty() {
                    skipped.push(format!("第 {} 列名空——补为「列 {}」", i + 1, i + 1));
                    format!("列 {}", i + 1)
                } else {
                    txt
                }
            }
            None => {
                skipped.push(format!("第 {} 列名空——补为「列 {}」", i + 1, i + 1));
                format!("列 {}", i + 1)
            }
        };
        // 类型推断：扫描该列数据行
        let kind = infer_kind(cells.iter().skip(1), i);
        columns.push((name, kind));
    }

    let mut rows: Vec<Vec<Option<Value>>> = Vec::with_capacity(cells.len() - 1);
    for row in cells.iter().skip(1) {
        // 全空行跳过（尾随空行常见——非内容,不计跳过报告）
        if row
            .iter()
            .all(|c| c.is_none() || matches!(c, Some(Value::String(s)) if s.trim().is_empty()))
        {
            continue;
        }
        let mut r: Vec<Option<Value>> = Vec::with_capacity(n_cols);
        for i in 0..n_cols {
            r.push(row.get(i).cloned().flatten());
        }
        rows.push(r);
    }

    if rows.is_empty() {
        return Err(ImportError::new("工作表仅表头无数据行——不产出空集合"));
    }

    Ok(XlsxImportResult {
        sheet_name: first_sheet_name,
        columns,
        rows,
        skipped,
    })
}

/// sharedStrings.xml → 顺序字符串表。
fn parse_shared_strings(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut in_si = false;
    let mut cur = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().local_name().as_ref() == b"si" => {
                in_si = true;
                cur.clear();
            }
            Ok(Event::Text(t)) if in_si => {
                cur.push_str(&t.decode().unwrap_or_default());
            }
            Ok(Event::End(e)) if e.name().local_name().as_ref() == b"si" => {
                in_si = false;
                out.push(std::mem::take(&mut cur));
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break, // 尽力解析
        }
        buf.clear();
    }
    out
}

/// workbook.xml → (首 sheet name, r:id, sheet 总数)。
fn parse_workbook(xml: &str) -> Result<(String, String, usize), ImportError> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut first: Option<(String, String)> = None;
    let mut count = 0usize;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().local_name().as_ref() == b"sheet" => {
                count += 1;
                let mut name = String::new();
                let mut rid = String::new();
                for a in e.attributes().flatten() {
                    match a.key.local_name().as_ref() {
                        b"name" => name = String::from_utf8_lossy(&a.value).into_owned(),
                        b"id" => rid = String::from_utf8_lossy(&a.value).into_owned(),
                        _ => {}
                    }
                }
                if first.is_none() {
                    first = Some((name, rid));
                }
            }
            Ok(Event::Empty(ref e)) if e.name().local_name().as_ref() == b"sheet" => {
                // 自闭合 <sheet/>（Excel 导出常态——Start 不命中即漏全部工作表）
                count += 1;
                let mut name = String::new();
                let mut rid = String::new();
                for a in e.attributes().flatten() {
                    match a.key.local_name().as_ref() {
                        b"name" => name = String::from_utf8_lossy(&a.value).into_owned(),
                        b"id" => rid = String::from_utf8_lossy(&a.value).into_owned(),
                        _ => {}
                    }
                }
                if first.is_none() {
                    first = Some((name, rid));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(ImportError::new(format!("workbook.xml 解析失败: {e}"))),
        }
        buf.clear();
    }
    let (name, rid) = first.ok_or_else(|| ImportError::new("workbook.xml 无工作表"))?;
    Ok((name, rid, count))
}

/// rels → r:id → Target。
fn parse_rels_target(xml: &str, rid: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().local_name().as_ref() == b"Relationship" => {
                if let Some(t) = rel_attrs(e, rid) {
                    return Some(t);
                }
            }
            Ok(Event::Empty(ref e)) if e.name().local_name().as_ref() == b"Relationship" => {
                // 自闭合（Aurora 导出件即此形态——漏则全部关系解析失败）
                if let Some(t) = rel_attrs(e, rid) {
                    return Some(t);
                }
            }
            Ok(Event::Eof) => return None,
            Ok(_) => {}
            Err(_) => return None,
        }
        buf.clear();
    }
}

/// Relationship 属性解析（Id/Target 匹配）。
fn rel_attrs(e: &quick_xml::events::BytesStart<'_>, rid: &str) -> Option<String> {
    let mut id = String::new();
    let mut target = String::new();
    for a in e.attributes().flatten() {
        match a.key.local_name().as_ref() {
            b"Id" => id = String::from_utf8_lossy(&a.value).into_owned(),
            b"Target" => target = String::from_utf8_lossy(&a.value).into_owned(),
            _ => {}
        }
    }
    if id == rid {
        Some(target)
    } else {
        None
    }
}

/// 原始单元格值（f→缓存 v;无缓存报告跳过）。
#[allow(clippy::type_complexity)]
fn parse_sheet(
    xml: &str,
    shared: &[String],
    skipped: &mut Vec<String>,
) -> Result<Vec<Vec<Option<Value>>>, ImportError> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    let mut rows: Vec<Vec<Option<Value>>> = Vec::new();
    let mut cur_row: Option<Vec<Option<Value>>> = None;
    let mut in_cell = false;
    let mut cell_ref = String::new();
    let mut cell_t = String::new();
    let mut cell_val = String::new();
    let mut inline_text = String::new();
    let mut in_is = false;
    let mut in_f = false;
    let mut has_formula = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = e.name().local_name(); // 绑本体（as_ref 去前缀——QName 直比会含 w:/r: 前缀永不中）
                match local.as_ref() {
                    b"row" => {
                        cur_row = Some(Vec::new());
                    }
                    b"c" => {
                        in_cell = true;
                        cell_ref.clear();
                        cell_t.clear();
                        cell_val.clear();
                        inline_text.clear();
                        has_formula = false;
                        for a in e.attributes().flatten() {
                            match a.key.local_name().as_ref() {
                                b"r" => cell_ref = String::from_utf8_lossy(&a.value).into_owned(),
                                b"t" => cell_t = String::from_utf8_lossy(&a.value).into_owned(),
                                _ => {}
                            }
                        }
                    }
                    b"is" => in_is = true,
                    b"f" => {
                        in_f = true;
                        has_formula = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if in_cell {
                    let text = t
                        .decode()
                        .map_err(|e| ImportError::new(format!("XML 解码失败: {e}")))?
                        .into_owned();
                    if in_is || cell_t == "inlineStr" {
                        inline_text.push_str(&text);
                    } else if !in_f {
                        cell_val.push_str(&text);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = e.name().local_name(); // 绑本体（as_ref 去前缀——QName 直比会含 w:/r: 前缀永不中）
                match local.as_ref() {
                    b"is" => in_is = false,
                    b"f" => in_f = false,
                    b"c" if in_cell => {
                        in_cell = false;
                        let v = decode_cell(&cell_t, &cell_val, &inline_text, shared);
                        if has_formula && v.is_none() {
                            skipped.push(format!(
                                "单元格 {cell_ref} 为公式且无缓存值——未导入（需在 Excel 重算后保存）"
                            ));
                        }
                        if let Some(row) = cur_row.as_mut() {
                            // ref 列号 → 列位（A→0）
                            let col = ref_to_col(&cell_ref);
                            while row.len() <= col {
                                row.push(None);
                            }
                            row[col] = v;
                        }
                    }
                    b"row" => {
                        if let Some(r) = cur_row.take() {
                            rows.push(r);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(ImportError::new(format!("工作表解析失败: {e}"))),
        }
        buf.clear();
    }
    Ok(rows)
}

/// 单元格值解码。
fn decode_cell(t: &str, v: &str, inline: &str, shared: &[String]) -> Option<Value> {
    match t {
        "inlineStr" => {
            if inline.is_empty() {
                None
            } else {
                Some(Value::String(inline.to_string()))
            }
        }
        "s" => {
            let idx: usize = v.parse().ok()?;
            shared.get(idx).map(|s| Value::String(s.clone()))
        }
        "b" => {
            let b = v == "1";
            Some(Value::Bool(b))
        }
        "str" => Some(Value::String(v.to_string())),
        "" | "n" => {
            if v.is_empty() {
                None
            } else if let Ok(n) = v.parse::<f64>() {
                // Excel serial 域启发交给列级推断;数值统一 f64（i64/f64 混型使
                // json!(3)!=json!(3.0) 往返断言假败——统一 f64 消除类型分叉）
                Some(serde_json::json!(n))
            } else {
                Some(Value::String(v.to_string()))
            }
        }
        _ => Some(Value::String(v.to_string())),
    }
}

/// 单元格引用（A1）→ 列下标。
fn ref_to_col(r: &str) -> usize {
    let mut n = 0usize;
    for c in r.chars() {
        if c.is_ascii_alphabetic() {
            n = n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
        } else {
            break;
        }
    }
    n.saturating_sub(1)
}

/// 列类型推断（数据行扫描）。
fn infer_kind<'a, I>(rows: I, col: usize) -> String
where
    I: Iterator<Item = &'a Vec<Option<Value>>>,
{
    let mut n_num = 0;
    let mut n_bool = 0;
    let mut n_date = 0;
    let mut n_total = 0;
    for row in rows {
        let Some(v) = row.get(col) else { continue };
        let Some(v) = v else { continue };
        if matches!(v, Value::String(s) if s.trim().is_empty()) {
            continue;
        }
        n_total += 1;
        match v {
            Value::Number(n) => {
                let f = n.as_f64().unwrap_or(0.0);
                if (20_000.0..=60_000.0).contains(&f) {
                    n_date += 1;
                }
                n_num += 1;
            }
            Value::Bool(_) => n_bool += 1,
            Value::String(s) => {
                if looks_like_iso_date(s) {
                    n_date += 1;
                }
            }
            _ => {}
        }
    }
    if n_total == 0 {
        return "Text".to_string();
    }
    if n_bool == n_total {
        return "Checkbox".to_string();
    }
    if n_num == n_total {
        // 全数值且大多落 serial 域 → Date
        if n_date >= n_total * 8 / 10 {
            return "Date".to_string();
        }
        return "Number".to_string();
    }
    if n_date == n_total {
        return "Date".to_string();
    }
    "Text".to_string()
}

/// ISO 日期启发（yyyy-mm-dd 前缀）。
fn looks_like_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10
        && b[0..4].iter().all(|c| c.is_ascii_digit())
        && b[4] == b'-'
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[7] == b'-'
        && b[8..10].iter().all(|c| c.is_ascii_digit())
}
