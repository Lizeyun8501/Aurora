//! DK-09 M3 集成测试 — HTML / Notion 导出 / OPML 导入。
//!
//! fixture 全部内联；写入走真实 bootstrap 装配 + write_path 唯一入口。
//! S5 教训：交付前 grep "test dk09" 逐一确认名单真实执行。

use std::path::Path;

use aurora_core::write_path::{load_note_meta, WriteContext};
use aurora_import::{
    import_html_file, import_notion_export, import_opml_file, HtmlImportOptions, OpmlImportOptions,
};

struct TestApp {
    _dir: tempfile::TempDir,
    ctx: WriteContext,
}

async fn test_app() -> TestApp {
    let dir = tempfile::tempdir().expect("tempdir");
    let booted = aurora_bootstrap::bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .expect("bootstrap");
    TestApp {
        _dir: dir,
        ctx: WriteContext {
            core: booted.core,
            blocks: booted.blocks,
            seal: None,
            attachments: None,
            content_cipher: None,
        },
    }
}

async fn read_back(app: &TestApp, note_id: &str) -> (String, String) {
    let record = load_note_meta(app.ctx.core.as_ref(), note_id, None)
        .await
        .expect("load_note_meta")
        .expect("note 存在");
    (record.title, record.content)
}

fn write_bytes(dir: &Path, rel: &str, data: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, data).unwrap();
    path
}

use std::path::PathBuf;

/// §M3-1 HTML 基础转换：标题/段落/列表/链接/表格 → Markdown。
#[tokio::test]
async fn dk09_m3_html_basic_conversion() {
    let app = test_app().await;
    let html = r#"<html><head><title>周报</title></head><body>
<h1>本周总结</h1>
<p>完成 <strong>导入器</strong> 开发，详见 <a href="https://example.com/docs">文档</a>。</p>
<ul><li>HTML</li><li>ENEX</li></ul>
<table><tr><th>项</th><th>状态</th></tr><tr><td>S1</td><td>done</td></tr></table>
</body></html>"#;
    let path = write_bytes(app._dir.path(), "weekly.html", html);

    let report = import_html_file(&app.ctx, &path, &HtmlImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 1);
    let (title, content) = read_back(&app, &report.note_ids[0]).await;
    assert_eq!(title, "周报", "标题取 <title> 元素");
    assert!(content.contains("# 本周总结"), "got: {content:?}");
    assert!(content.contains("**导入器**"), "got: {content:?}");
    assert!(
        content.contains("[文档](https://example.com/docs)"),
        "got: {content:?}"
    );
    assert!(content.contains("- HTML"), "got: {content:?}");
    assert!(content.contains("| 项 | 状态 |"), "got: {content:?}");
    assert!(content.contains("|---|---|"), "got: {content:?}");
    assert!(content.contains("| S1 | done |"), "got: {content:?}");
}

/// §M3-2 HTML 实体解码：预定义实体 + 数字字符引用逐字符还原。
#[tokio::test]
async fn dk09_m3_html_entity_decoding() {
    let app = test_app().await;
    let html =
        "<html><body><p>A &amp; B &lt; C &gt; D &#20013;&#25991; &quot;q&quot;</p></body></html>";
    let path = write_bytes(app._dir.path(), "ent.html", html);
    let report = import_html_file(&app.ctx, &path, &HtmlImportOptions::default())
        .await
        .expect("import ok");
    let (_, content) = read_back(&app, &report.note_ids[0]).await;
    assert!(
        content.contains("A & B < C > D 中文 \"q\""),
        "got: {content:?}"
    );
}

/// §M3-3 HTML img：资源本体不在 → 链接保留 + warning。
#[tokio::test]
async fn dk09_m3_html_img_kept_with_warning() {
    let app = test_app().await;
    let html = "<html><body><img src=\"pic.png\" alt=\"示意图\"/></body></html>";
    let path = write_bytes(app._dir.path(), "img.html", html);
    let report = import_html_file(&app.ctx, &path, &HtmlImportOptions::default())
        .await
        .expect("import ok");
    assert!(!report.warnings.is_empty(), "img 应记 warning");
    let (_, content) = read_back(&app, &report.note_ids[0]).await;
    assert!(content.contains("![示意图](pic.png)"), "got: {content:?}");
}

/// §M3-4 HTML 非良构：failed 记账（不产生半成品）。
#[tokio::test]
async fn dk09_m3_html_malformed_fails() {
    let app = test_app().await;
    let html = "<html><body><p>未闭合";
    let path = write_bytes(app._dir.path(), "bad.html", html);
    let err = import_html_file(&app.ctx, &path, &HtmlImportOptions::default())
        .await
        .expect_err("非良构 HTML 必须报错");
    assert!(err.reason.contains("HTML 转换失败"), "got: {err}");
}

/// §M3-5 Notion 导出：文件名 hash 后缀剥离 + md 正文无损 + csv 跳过。
#[tokio::test]
async fn dk09_m3_notion_export_import() {
    let app = test_app().await;
    let export_root = app._dir.path().join("notion");
    // Notion 文件名约定：标题 + 空格 + 32 位 hex
    let hex = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6";
    write_bytes(
        &export_root,
        &format!("项目规划 {hex}.md"),
        "# 项目规划\n\n里程碑如下\n",
    );
    write_bytes(
        &export_root,
        &format!("阅读清单 {hex}.md"),
        "阅读清单\n\n- 书1\n- 书2\n",
    );
    // 数据库 CSV → 跳过
    write_bytes(&export_root, &format!("任务库 {hex}.csv"), "Name,Status\n");

    let report = import_notion_export(&app.ctx, &export_root)
        .await
        .expect("import ok");
    assert_eq!(report.scanned, 2, "csv 不算候选");
    assert_eq!(report.imported, 2);
    assert_eq!(report.skipped, 1);
    assert!(!report.warnings.is_empty(), "csv 数据库应记 warning");

    let titles: Vec<String> = report.entries.iter().map(|e| e.title.clone()).collect();
    assert!(titles.contains(&"项目规划".to_string()), "{titles:?}");
    assert!(titles.contains(&"阅读清单".to_string()), "{titles:?}");

    // 正文读回无损（含 frontmatter 通道之外的普通 md）
    for entry in &report.entries {
        let (title, content) = read_back(&app, &entry.note_id).await;
        if title == "项目规划" {
            assert_eq!(content, "# 项目规划\n\n里程碑如下\n");
        }
        if title == "阅读清单" {
            assert!(content.contains("- 书1") && content.contains("- 书2"));
        }
    }
}

/// §M3-6 OPML：顶层 outline → 一篇笔记，子树转嵌套列表，_note 为正文行。
#[tokio::test]
async fn dk09_m3_opml_outline_to_note() {
    let app = test_app().await;
    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head><title>幕布导出</title></head>
  <body>
    <outline text="产品路线">
      <outline text="Q3" _note="冲刺期">
        <outline text="导入器"/>
        <outline text="同步"/>
      </outline>
      <outline text="Q4"/>
    </outline>
    <outline text="读书笔记">
      <outline text="第一章" _note="要点若干"/>
    </outline>
  </body>
</opml>"#;
    let path = write_bytes(app._dir.path(), "outline.opml", opml);
    let report = import_opml_file(&app.ctx, &path, &OpmlImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.scanned, 2);
    assert_eq!(report.imported, 2);

    let mut got: Vec<(String, String)> = Vec::new();
    for e in &report.entries {
        got.push(read_back(&app, &e.note_id).await);
    }
    got.sort_by(|a, b| a.0.cmp(&b.0));
    // 字节序：产(E4) < 读(E8) → 产品路线在前
    assert_eq!(got[0].0, "产品路线");
    assert!(got[0].1.contains("- Q3"), "got: {:?}", got[0].1);
    assert!(got[0].1.contains("  - 导入器"), "got: {:?}", got[0].1);
    assert!(got[0].1.contains("  - 同步"), "got: {:?}", got[0].1);
    assert!(got[0].1.contains("- Q4"), "got: {:?}", got[0].1);
    assert_eq!(got[1].0, "读书笔记");
    assert!(got[1].1.contains("- 第一章"), "got: {:?}", got[1].1);
    assert!(got[1].1.contains("要点若干"), "got: {:?}", got[1].1);
}

/// §M3-7 OPML 损坏 XML：致命错误。
#[tokio::test]
async fn dk09_m3_opml_invalid_xml_is_fatal() {
    let app = test_app().await;
    let path = write_bytes(
        app._dir.path(),
        "bad.opml",
        "<opml><body><outline text=\"x\">",
    );
    let err = import_opml_file(&app.ctx, &path, &OpmlImportOptions::default())
        .await
        .expect_err("损坏 OPML 必须报错");
    assert!(err.reason.contains("OPML 解析失败"), "got: {err}");
}

/// §M3-8 HTML 空文档：无 title/h1 → 文件名 stem 兜底。
#[tokio::test]
async fn dk09_m3_html_empty_falls_back_to_stem() {
    let app = test_app().await;
    let path = write_bytes(app._dir.path(), "空白页.html", "<html><body></body></html>");
    let report = import_html_file(&app.ctx, &path, &HtmlImportOptions::default())
        .await
        .expect("import ok");
    let (title, _) = read_back(&app, &report.note_ids[0]).await;
    assert_eq!(title, "空白页");
}
