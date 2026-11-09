//! A-01 往返测试：DOCX / XLSX 导出 → 导入 → 结构比对（含损失清单断言）。

use aurora_import::docx_export::markdown_to_docx;
use aurora_import::docx_import::docx_to_markdown;
use aurora_import::xlsx_export::{collection_to_xlsx, XlsxColumn, XlsxSheet};
use aurora_import::xlsx_import::xlsx_to_collection;
use serde_json::json;

/// DOCX: 结构断言（zip 可解 + document.xml 关键块 + 中文不丢）。
#[test]
fn docx_export_structure() {
    let md = "# 标题一\n\n正文段**加粗**与`代码`。\n\n- 列表甲\n- 列表乙\n\n1. 有序一\n2. 有序二\n\n- [ ] 待办\n- [x] 已办\n\n> 引用行\n\n```rust\nfn main() {}\n```\n\n| 列A | 列B |\n| --- | --- |\n| 值1 | 值2 |\n";
    let bytes = markdown_to_docx("测试笔记", md).expect("导出失败");
    assert!(bytes.len() > 2000, "docx 过小: {}", bytes.len());

    let cursor = std::io::Cursor::new(&bytes);
    let mut zip = zip::ZipArchive::new(cursor).expect("zip 解包失败");
    let mut xml = String::new();
    {
        let mut f = zip.by_name("word/document.xml").expect("无 document.xml");
        use std::io::Read;
        f.read_to_string(&mut xml).unwrap();
    }
    // 关键块断言
    assert!(xml.contains("Heading1"), "缺标题样式引用");
    assert!(xml.contains("标题一"), "标题文本丢");
    assert!(xml.contains("加粗"), "粗体文本丢");
    assert!(xml.contains("列表甲"), "列表文本丢");
    assert!(xml.contains("☐"), "任务勾选态丢");
    assert!(xml.contains("☒"), "任务已勾丢");
    assert!(xml.contains("Consolas"), "代码块等宽字体丢");
    assert!(xml.contains("<w:tbl>"), "表格丢");
    assert!(xml.contains("列A"), "表头文本丢");
    assert!(xml.contains("引用行"), "引用文本丢");
    assert!(xml.contains("宋体"), "东亚字体声明丢（中文乱码风险）");
    assert!(xml.contains("正文段"), "正文丢");
}

/// DOCX 往返：导出 → 导入 → 核心语义比对（允许格式损失——列损失清单）。
#[test]
fn docx_roundtrip_semantics() {
    let md = "# 项目纪要\n\n第一个正文段。\n\n- 事项甲\n- 事项乙\n\n- [ ] 未完成项\n- [x] 已完成项\n\n> 重要引述\n\n```rust\nlet x = 1;\n```\n\n| 姓名 | 分数 |\n| --- | --- |\n| 张三 | 95 |\n| 李四 | 88 |\n";
    let bytes = markdown_to_docx("纪要", md).expect("导出失败");
    let back = docx_to_markdown(&bytes).expect("导入失败");

    // 语义保留断言（格式允许损失）
    assert!(
        back.markdown.contains("# 项目纪要"),
        "标题级语义丢: {}",
        back.markdown
    );
    assert!(back.markdown.contains("第一个正文段"), "正文丢");
    assert!(
        back.markdown.contains("事项甲") && back.markdown.contains("事项乙"),
        "列表项丢"
    );
    assert!(
        back.markdown.contains("未完成项") && back.markdown.contains("已完成项"),
        "任务项丢"
    );
    assert!(back.markdown.contains("重要引述"), "引用丢");
    assert!(back.markdown.contains("let x = 1;"), "代码内容丢");
    assert!(
        back.markdown.contains("张三") && back.markdown.contains("95"),
        "表格内容丢"
    );
    assert!(
        back.markdown.contains("姓名") && back.markdown.contains("分数"),
        "表头丢"
    );
    // 损失清单披露（本往返设计内损失——不为空才证明报告机制活着）
    // v1: 链接降级文本/列表非真 Word 列表——此处 MD 源无链接,skipped 应为空
    assert!(back.skipped.is_empty(), "意外跳过项: {:?}", back.skipped);
}

/// DOCX 导入：跳过报告（图片不静默丢）。
#[test]
fn docx_import_skips_reported() {
    // 手造最小 docx: document.xml 含一个 drawing（图片占位）
    let doc = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>有图段落</w:t></w:r></w:p>
<w:p><w:r><w:drawing><wp:inline xmlns:wp="x"><a:graphic xmlns:a="y"/></wp:inline></w:drawing></w:r></w:p>
</w:body></w:document>"#;
    let styles = r#"<?xml version="1.0" encoding="UTF-8"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;
    let bytes = {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            use std::io::Write;
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
            zip.start_file("[Content_Types].xml", opts).unwrap();
            zip.write_all(
                br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#,
            )
            .unwrap();
            zip.start_file("_rels/.rels", opts).unwrap();
            zip.write_all(b"<R/>").unwrap();
            zip.start_file("word/document.xml", opts).unwrap();
            zip.write_all(doc.as_bytes()).unwrap();
            zip.start_file("word/styles.xml", opts).unwrap();
            zip.write_all(styles.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        buf.into_inner()
    };
    let res = docx_to_markdown(&bytes).expect("导入失败");
    assert!(res.markdown.contains("有图段落"));
    assert!(
        res.skipped.iter().any(|s| s.contains("图片")),
        "图片未进跳过报告: {:?}",
        res.skipped
    );
}

/// XLSX: 结构断言（列类型映射 + 中文 + 公式批注）。
#[test]
fn xlsx_export_structure() {
    let sheet = XlsxSheet {
        sheet_name: "项目集".into(),
        columns: vec![
            XlsxColumn {
                name: "名称".into(),
                prop_kind: "Text".into(),
                formula: None,
            },
            XlsxColumn {
                name: "数量".into(),
                prop_kind: "Number".into(),
                formula: None,
            },
            XlsxColumn {
                name: "到期日".into(),
                prop_kind: "Date".into(),
                formula: None,
            },
            XlsxColumn {
                name: "完成".into(),
                prop_kind: "Checkbox".into(),
                formula: None,
            },
            XlsxColumn {
                name: "合计 = 数量*2".into(),
                prop_kind: "Formula".into(),
                formula: Some("数量*2".into()),
            },
        ],
        rows: vec![
            vec![
                Some(json!("甲任务")),
                Some(json!(3)),
                Some(json!("2026-11-09")),
                Some(json!(true)),
                Some(json!(6)),
            ],
            vec![
                Some(json!("乙任务")),
                Some(json!(5)),
                Some(json!("2026-12-01")),
                Some(json!(false)),
                Some(json!(10)),
            ],
        ],
    };
    let bytes = collection_to_xlsx(&sheet).expect("导出失败");
    let cursor = std::io::Cursor::new(bytes.clone()); // 借期解耦（ZipFile<'a> 尾块 drop 竞态规避）
    let mut zip = zip::ZipArchive::new(cursor).expect("zip 解包失败");
    let mut sheet1 = String::new();
    {
        let mut f = zip.by_name("xl/worksheets/sheet1.xml").expect("无 sheet1");
        use std::io::Read;
        f.read_to_string(&mut sheet1).unwrap();
    }
    {
        // sheet 名在 workbook.xml（sheet1.xml 只有单元格——断言错位修）
        let mut f = zip.by_name("xl/workbook.xml").expect("无 workbook");
        use std::io::Read;
        let mut wb = String::new();
        f.read_to_string(&mut wb).unwrap();
        assert!(wb.contains("项目集"), "工作表名丢");
    }
    assert!(
        sheet1.contains("甲任务") && sheet1.contains("乙任务"),
        "中文行值丢"
    );
    assert!(sheet1.contains("t=\"inlineStr\""), "文本单元格类型丢");
    assert!(sheet1.contains("<v>3</v>"), "数值单元格丢");
    assert!(sheet1.contains("t=\"b\""), "布尔单元格类型丢");
    assert!(sheet1.contains("s=\"1\""), "日期样式引用丢");
    // 公式批注
    assert!(zip.by_name("xl/comments1.xml").is_ok(), "公式列批注部件丢");
    let mut c = match zip.by_name("xl/comments1.xml") {
        Ok(c) => c,
        Err(_) => panic!("公式列批注部件丢"),
    };
    let mut xml = String::new();
    {
        use std::io::Read;
        c.read_to_string(&mut xml).unwrap();
    }
    drop(c); // ZipFile 借 zip 显式释放（E0597 尾块竞态规避）
    assert!(xml.contains("公式列"), "批注文案丢: {}", xml);
    assert!(xml.contains("数量*2"), "批注表达式丢");
}

/// XLSX 往返：导出 → 导入 → schema 推断 + 行值比对。
#[test]
fn xlsx_roundtrip_semantics() {
    let sheet = XlsxSheet {
        sheet_name: "清单".into(),
        columns: vec![
            XlsxColumn {
                name: "名称".into(),
                prop_kind: "Text".into(),
                formula: None,
            },
            XlsxColumn {
                name: "数量".into(),
                prop_kind: "Number".into(),
                formula: None,
            },
            XlsxColumn {
                name: "完成".into(),
                prop_kind: "Checkbox".into(),
                formula: None,
            },
        ],
        rows: vec![
            vec![Some(json!("甲")), Some(json!(3)), Some(json!(true))],
            vec![Some(json!("乙")), Some(json!(7)), Some(json!(false))],
        ],
    };
    let bytes = collection_to_xlsx(&sheet).expect("导出失败");
    let back = xlsx_to_collection(&bytes).expect("导入失败");

    assert_eq!(back.sheet_name, "清单");
    assert_eq!(back.columns.len(), 3, "列数变: {:?}", back.columns);
    assert_eq!(back.columns[0].0, "名称");
    // 类型推断断言
    assert_eq!(
        back.columns[1].1, "Number",
        "数值列推断错: {:?}",
        back.columns
    );
    assert_eq!(
        back.columns[2].1, "Checkbox",
        "布尔列推断错: {:?}",
        back.columns
    );
    // 行值
    assert_eq!(back.rows.len(), 2);
    assert_eq!(back.rows[0][0], Some(json!("甲")));
    assert_eq!(back.rows[0][1], Some(json!(3.0)));
    assert_eq!(back.rows[1][2], Some(json!(false)));
}

/// XLSX 导入：空表明确报错（禁止静默占位）。
#[test]
fn xlsx_import_empty_rejected() {
    // 手造空 sheet（仅表头行也接受——无数据行才拒）
    let sheet1 = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#;
    let bytes = {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            use std::io::Write;
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
            zip.start_file("[Content_Types].xml", opts).unwrap();
            zip.write_all(b"<T/>").unwrap();
            zip.start_file("_rels/.rels", opts).unwrap();
            zip.write_all(b"<R/>").unwrap();
            zip.start_file("xl/workbook.xml", opts).unwrap();
            zip.write_all(br#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S1" r:id="rId1"/></sheets></workbook>"#).unwrap();
            zip.start_file("xl/_rels/workbook.xml.rels", opts).unwrap();
            zip.write_all(br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#).unwrap();
            zip.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
            zip.write_all(sheet1.as_bytes()).unwrap();
            zip.start_file("xl/styles.xml", opts).unwrap();
            zip.write_all(b"<s/>").unwrap();
            zip.finish().unwrap();
        }
        buf.into_inner()
    };
    let err = match xlsx_to_collection(&bytes) {
        Err(e) => e,
        Ok(_) => panic!("空表应报错（禁止静默占位）"),
    };
    assert!(
        err.to_string().contains("空表") || err.to_string().contains("无数据"),
        "错误语义不对: {err}"
    );
}
