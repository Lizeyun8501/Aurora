//! DK-09 迁移向导内核集成测试 — 预扫 / 防重 / 进度 / 选择性导入。
//!
//! S5 教训：交付前 grep "test dk09" 逐一确认名单真实执行。

use std::path::Path;

use aurora_core::write_path::{load_note_meta, WriteContext};
use aurora_import::{
    import_enex, import_markdown_dir, plan_enex_file, plan_markdown_dir, plan_opml_file,
    EnexImportOptions, ImportOptions, ProgressEvent,
};
use tokio::sync::mpsc::unbounded_channel;

struct TestApp {
    _dir: tempfile::TempDir,
    ctx: WriteContext,
}

async fn test_app() -> TestApp {
    let dir = tempfile::tempdir().expect("tempdir");
    let booted = aurora_bootstrap::bootstrap(dir.path()).expect("bootstrap");
    TestApp {
        _dir: dir,
        ctx: WriteContext {
            core: booted.core,
            blocks: booted.blocks,
            seal: None,
            content_cipher: None,
        },
    }
}

fn write_bytes(root: &Path, rel: &str, data: &str) -> PathBuf {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, data).unwrap();
    path
}

use std::path::PathBuf;

/// §W-1 预扫 markdown：条目数/标题/指纹稳定，且与 import 报告自洽。
#[tokio::test]
async fn dk09_wizard_plan_markdown_stable() {
    let app = test_app().await;
    let src = app._dir.path().join("src");
    write_bytes(&src, "a.md", "# 甲\n内容");
    write_bytes(&src, "b.md", "## 乙\n内容乙");
    write_bytes(&src, "notes/c.md", "# 丙\n子目录");

    let p1 = plan_markdown_dir(&src, &ImportOptions::default()).expect("plan");
    assert_eq!(p1.items.len(), 3);
    assert_eq!(p1.kind, aurora_import::ImportKind::Markdown);
    // 标题语义与 import 一致：无 frontmatter 时 = 文件名 stem
    assert!(p1.items.iter().any(|i| i.title == "a"));
    assert!(
        p1.items.iter().any(|i| i.title == "c"),
        "子目录文件也入计划"
    );
    // 指纹稳定：两次 plan 同 hash
    let p2 = plan_markdown_dir(&src, &ImportOptions::default()).expect("plan");
    for (i1, i2) in p1.items.iter().zip(&p2.items) {
        assert_eq!(i1.content_hash, i2.content_hash);
        assert_eq!(i1.content_hash.len(), 16, "sha256 前 16 hex");
    }

    // plan 与 import 一致：按 plan 数量写入
    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import");
    assert_eq!(report.imported as usize, p1.items.len());
}

/// §W-2 预扫 enex：每 note 一条，resources 计数正确。
#[tokio::test]
async fn dk09_wizard_plan_enex_counts_resources() {
    let app = test_app().await;
    let enex = r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export export-date="20240101T000000Z" application="Evernote">
  <note><title>带图笔记</title><content><![CDATA[<?xml version="1.0" encoding="UTF-8"?><en-note><p>正文</p><en-media type="image/png" hash="abc123def456"/></en-note>]]></content><resource><mime>image/png</mime><data encoding="base64">aGVsbG8=</data></resource></note>
  <note><title>纯文本</title><content><![CDATA[<?xml version="1.0" encoding="UTF-8"?><en-note><p>无资源</p></en-note>]]></content></note>
</en-export>"#;
    let path = write_bytes(app._dir.path(), "export.enex", enex);
    let plan = plan_enex_file(&path).expect("plan");
    assert_eq!(plan.kind, aurora_import::ImportKind::Enex);
    assert_eq!(plan.items.len(), 2);
    let with_res = plan.items.iter().find(|i| i.title == "带图笔记").unwrap();
    assert_eq!(with_res.resources, 1);
    let plain = plan.items.iter().find(|i| i.title == "纯文本").unwrap();
    assert_eq!(plain.resources, 0);
}

/// §W-3 预扫 opml：每顶层 outline 一条。
#[tokio::test]
async fn dk09_wizard_plan_opml_roots() {
    let app = test_app().await;
    let opml = r#"<?xml version="1.0"?><opml version="2.0"><body>
      <outline text="甲树"><outline text="child"/></outline>
      <outline text="乙树"/>
    </body></opml>"#;
    let path = write_bytes(app._dir.path(), "o.opml", opml);
    let plan = plan_opml_file(&path).expect("plan");
    assert_eq!(plan.items.len(), 2);
    assert_eq!(plan.items[0].title, "甲树");
    assert_eq!(plan.items[0].bytes, 2, "nodes 计数 = 子树节点数");
    assert_eq!(plan.items[1].bytes, 1);
}

/// §W-4 manifest 防重：同清单二次导入全部 skip，note_id 可追溯。
#[tokio::test]
async fn dk09_wizard_manifest_prevents_duplicate() {
    let app = test_app().await;
    let src = app._dir.path().join("src");
    write_bytes(&src, "a.md", "# 甲\n内容");
    write_bytes(&src, "b.md", "# 乙\n内容乙");
    let manifest_dir = app._dir.path().join("imports").join("t1");

    let opts = ImportOptions {
        manifest_dir: Some(manifest_dir.clone()),
        ..Default::default()
    };
    let r1 = import_markdown_dir(&app.ctx, &src, &opts)
        .await
        .expect("r1");
    assert_eq!(r1.imported, 2);
    assert_eq!(r1.skipped, 0);

    let r2 = import_markdown_dir(&app.ctx, &src, &opts)
        .await
        .expect("r2");
    assert_eq!(r2.imported, 0, "二次导入全部防重跳过");
    assert_eq!(r2.skipped, 2);
    assert_eq!(r2.scanned, 2, "scanned 仍计应处理条目");

    // manifest.json 持久化且条目可追溯
    let raw = std::fs::read_to_string(manifest_dir.join("manifest.json")).unwrap();
    assert!(raw.contains("\"source\"") && raw.contains("\"note_id\""));

    // 内容变更 → 同路径重新导入（防重键含内容指纹）
    write_bytes(&src, "a.md", "# 甲\n内容 v2");
    let r3 = import_markdown_dir(&app.ctx, &src, &opts)
        .await
        .expect("r3");
    assert_eq!(r3.imported, 1, "变更内容重新导入");
    assert_eq!(r3.skipped, 1, "未变更的 b.md 仍跳过");
}

/// §W-5 进度推送：total 与 final 事件、current 单调递增。
#[tokio::test]
async fn dk09_wizard_progress_events() {
    let app = test_app().await;
    let src = app._dir.path().join("src");
    write_bytes(&src, "1.md", "# 一");
    write_bytes(&src, "2.md", "# 二");
    write_bytes(&src, "3.md", "# 三");
    let (tx, mut rx) = unbounded_channel::<ProgressEvent>();
    let opts = ImportOptions {
        progress: Some(tx),
        ..Default::default()
    };
    let report = import_markdown_dir(&app.ctx, &src, &opts)
        .await
        .expect("ok");
    assert_eq!(report.imported, 3);

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert_eq!(events.len(), 3, "每文件一条（无 manifest skip）");
    let last = events.last().unwrap();
    assert_eq!(last.total, 3);
    assert_eq!(last.current, 3);
    for (i, e) in events.iter().enumerate() {
        assert_eq!(e.current, (i + 1) as u32, "current 单调 1-based");
    }
}

/// §W-6 选择性导入（only）：markdown 相对路径 + 目录前缀。
#[tokio::test]
async fn dk09_wizard_only_filter_markdown() {
    let app = test_app().await;
    let src = app._dir.path().join("src");
    write_bytes(&src, "a.md", "# 甲");
    write_bytes(&src, "sub/b.md", "# 乙");
    write_bytes(&src, "sub/c.md", "# 丙");
    let opts = ImportOptions {
        only: vec!["a.md".into(), "sub".into()],
        ..Default::default()
    };
    let report = import_markdown_dir(&app.ctx, &src, &opts)
        .await
        .expect("ok");
    assert_eq!(report.imported, 3, "精确 + 目录前缀命中");
    assert_eq!(report.entries[0].source, "a.md", "source 记相对路径");
}

/// §W-7 选择性导入（only）+ 防重组合：enex `<file>#<idx>` 键。
#[tokio::test]
async fn dk09_wizard_enex_only_and_manifest() {
    let app = test_app().await;
    let enex = r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export application="Evernote">
  <note><title>一</title><content><![CDATA[<?xml version="1.0"?><en-note><p>1</p></en-note>]]></content></note>
  <note><title>二</title><content><![CDATA[<?xml version="1.0"?><en-note><p>2</p></en-note>]]></content></note>
</en-export>"#;
    let path = write_bytes(app._dir.path(), "e.enex", enex);
    let manifest_dir = app._dir.path().join("imports").join("t2");

    let opts = EnexImportOptions {
        only: vec!["e.enex#1".into()],
        manifest_dir: Some(manifest_dir),
        ..Default::default()
    };
    let r1 = import_enex(&app.ctx, &path, &opts).await.expect("r1");
    assert_eq!(r1.imported, 1, "only 命中第二篇");
    let (title, _) = read_back(&app, &r1.note_ids[0]).await;
    assert_eq!(title, "二");

    let r2 = import_enex(&app.ctx, &path, &opts).await.expect("r2");
    assert_eq!(r2.imported, 0);
    assert_eq!(r2.skipped, 1, "同 (source,hash) 防重");
}

async fn read_back(app: &TestApp, note_id: &str) -> (String, String) {
    let record = load_note_meta(app.ctx.core.as_ref(), note_id, None)
        .await
        .expect("load_note_meta")
        .expect("note 存在");
    (record.title, String::new())
}

/// §W-8 进度收敛不变量：failed 条目也必须推进度（current 终达 total）。
#[tokio::test]
async fn dk09_wizard_progress_converges_on_failure() {
    let app = test_app().await;
    // 第二篇 ENML 未闭合 → enml_to_markdown Err → failed 分支
    let enex = r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export application="Evernote">
  <note><title>好笔记</title><content><![CDATA[<?xml version="1.0"?><en-note><p>ok</p></en-note>]]></content></note>
  <note><title>坏笔记</title><content><![CDATA[<?xml version="1.0"?><en-note><p>x</div></en-note>]]></content></note>
</en-export>"#;
    let path = write_bytes(app._dir.path(), "mix.enex", enex);
    let (tx, mut rx) = unbounded_channel();
    let opts = EnexImportOptions {
        progress: Some(tx),
        ..Default::default()
    };
    let report = import_enex(&app.ctx, &path, &opts).await.expect("ok");
    assert_eq!(report.imported, 1);
    assert_eq!(report.failed, 1, "未闭合 ENML 记 failed");
    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }
    assert_eq!(events.len(), 2, "每条目恰好一条进度: {events:?}");
    let last = events.last().expect("事件非空");
    assert_eq!((last.current, last.total), (2, 2), "终态 current=total");
}

/// §W-9 OPML 向导能力：进度 total/收敛 + manifest 防重 + only。
#[tokio::test]
async fn dk09_wizard_opml_progress_and_manifest() {
    let app = test_app().await;
    let opml = r#"<?xml version="1.0"?><opml version="2.0"><body>
      <outline text="甲"><outline text="a1"/></outline>
      <outline text="乙"/>
      <outline text="丙"><outline text="c1"/><outline text="c2"/></outline>
    </body></opml>"#;
    let path = write_bytes(app._dir.path(), "w.opml", opml);
    let manifest_dir = app._dir.path().join("imports").join("t9");

    let (tx, mut rx) = unbounded_channel();
    let opts = aurora_import::OpmlImportOptions {
        manifest_dir: Some(manifest_dir.clone()),
        progress: Some(tx),
        only: Vec::new(),
    };
    let r1 = aurora_import::import_opml_file(&app.ctx, &path, &opts)
        .await
        .expect("r1");
    assert_eq!(r1.imported, 3);
    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }
    assert_eq!(events.len(), 3);
    assert_eq!((events[2].current, events[2].total), (3, 3));

    // 二次导入：全部防重 skip
    let r2 = aurora_import::import_opml_file(&app.ctx, &path, &opts)
        .await
        .expect("r2");
    assert_eq!(r2.imported, 0);
    assert_eq!(r2.skipped, 3);
    assert_eq!(r2.scanned, 3, "scanned = 过滤后应处理数");

    // only 白名单：<文件名>#<idx>
    let only = aurora_import::OpmlImportOptions {
        only: vec!["w.opml#2".into()],
        ..Default::default()
    };
    let r3 = aurora_import::import_opml_file(&app.ctx, &path, &only)
        .await
        .expect("r3");
    assert_eq!(r3.imported, 1);
    assert_eq!(r3.entries[0].title, "丙");
}
