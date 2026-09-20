//! DK-09 S2 集成测试 — ENEX（印象笔记导出）解析导入。
//!
//! fixture 为内联 XML 常量（Evernote Export DTD 3 结构）；写入走真实
//! bootstrap 装配 + write_path 唯一入口；资源 base64 往返逐字节断言。
//! S5 教训：交付前 `cargo test -p aurora-import 2>&1 | grep "test dk09"`
//! 逐一确认名单真实执行。

use std::path::PathBuf;

use aurora_core::write_path::{load_note_meta, WriteContext};
use aurora_import::{import_enex, EnexImportOptions};

/// 真实装配的测试上下文。
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
            seal: None,           // 明文落盘
            content_cipher: None, // DK-07 S2：none 级不触 cipher
        },
    }
}

/// 写 ENEX 文件并返回路径。
fn write_enex(dir: &std::path::Path, name: &str, xml: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, xml).expect("write enex");
    path
}

/// 读回笔记（title, content）。
async fn read_back(app: &TestApp, note_id: &str) -> (String, String) {
    let record = load_note_meta(app.ctx.core.as_ref(), note_id, None)
        .await
        .expect("load_note_meta")
        .expect("note exists");
    (record.title, record.content)
}

/// 4 字节 PNG 魔数前缀的样例资源字节。
const RESOURCE_BYTES: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn b64(data: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// 最小合法 ENEX（1 note，无资源）。
const ENEX_MINIMAL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export application="Evernote" version="0.85">
  <note>
    <title>会议纪要</title>
    <content><![CDATA[<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!DOCTYPE en-note SYSTEM "http://xml.evernote.com/pub/enml2.dtd">
<en-note><h1>会议纪要</h1><div>周一例会 &amp; 排期确认</div></en-note>]]></content>
    <created>20260101T090000Z</created>
    <updated>20260102T100000Z</updated>
    <tag>工作</tag>
    <tag>周会</tag>
  </note>
</en-export>"#;

/// §S2-1 单 note 基础导入：标题/正文/实体解码。
#[tokio::test]
async fn dk09_enex_single_note_basic() {
    let app = test_app().await;
    let path = write_enex(app._dir.path(), "notes.enex", ENEX_MINIMAL);
    let report = import_enex(&app.ctx, &path, &EnexImportOptions::default())
        .await
        .expect("import ok");

    assert_eq!(report.scanned, 1);
    assert_eq!(report.imported, 1);
    assert_eq!(report.failed, 0);
    let (title, content) = read_back(&app, &report.note_ids[0]).await;
    assert_eq!(title, "会议纪要");
    // h1 同行 + 实体 &amp; 解码为 &（quick-xml GeneralRef 臂）
    assert!(content.contains("# 会议纪要"), "got: {content:?}");
    assert!(content.contains("周一例会 & 排期确认"), "got: {content:?}");
}

/// §S2-2 多 note + 标签 + entries 明细。
#[tokio::test]
async fn dk09_enex_multiple_notes_and_tags() {
    let app = test_app().await;
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export version="0.85">
  <note><title>A</title><content><![CDATA[<en-note><div>甲</div></en-note>]]></content><tag>t1</tag></note>
  <note><title>B</title><content><![CDATA[<en-note><div>乙</div></en-note>]]></content><tag>t2</tag><tag>t3</tag></note>
</en-export>"#;
    let path = write_enex(app._dir.path(), "multi.enex", xml);
    let report = import_enex(&app.ctx, &path, &EnexImportOptions::default())
        .await
        .expect("import ok");

    assert_eq!(report.scanned, 2);
    assert_eq!(report.imported, 2);
    assert_eq!(report.entries.len(), 2);
    let titles: Vec<String> = report.entries.iter().map(|e| e.title.clone()).collect();
    assert!(titles.contains(&"A".to_string()) && titles.contains(&"B".to_string()));
    let tags_b = &report.entries[1].tags;
    assert_eq!(tags_b, &vec!["t2".to_string(), "t3".to_string()]);
    let (_, c0) = read_back(&app, &report.note_ids[0]).await;
    assert!(c0.contains("甲"));
}

/// §S2-3 资源提取：base64 往返逐字节一致 + en-media 占位链接 + entries。
#[tokio::test]
async fn dk09_enex_resource_sidecar_written() {
    let app = test_app().await;
    let hash = "aabbccdd00112233445566778899aabb";
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export version="0.85">
  <note><title>带图笔记</title>
    <content><![CDATA[<en-note><div>看图</div><en-media type="image/png" hash="{hash}"></en-media></en-note>]]></content>
    <resource>
      <data encoding="base64" hash="{hash}">{b64}</data>
      <mime>image/png</mime>
      <resource-attributes><file-name>截图.png</file-name></resource-attributes>
    </resource>
  </note>
</en-export>"#,
        b64 = b64(RESOURCE_BYTES)
    );
    let path = write_enex(app._dir.path(), "res.enex", &xml);
    let att_dir = app._dir.path().join("attachments");
    let report = import_enex(
        &app.ctx,
        &path,
        &EnexImportOptions {
            attachments_dir: Some(att_dir.clone()),
        },
    )
    .await
    .expect("import ok");

    assert_eq!(report.imported, 1);
    // sidecar 文件：hash12-截图.png，字节逐一对齐
    let entry = &report.entries[0];
    assert_eq!(entry.resources.len(), 1);
    let written = entry.resources[0].written_to.as_ref().expect("落盘");
    assert!(written.starts_with(&att_dir));
    assert_eq!(
        std::fs::read(written).expect("read sidecar"),
        RESOURCE_BYTES,
        "base64 往返必须逐字节一致"
    );
    assert_eq!(entry.resources[0].hash, hash);
    assert_eq!(entry.resources[0].file_name.as_deref(), Some("截图.png"));
    // 正文占位指向 sidecar 文件名
    let (_, content) = read_back(&app, &report.note_ids[0]).await;
    let sidecar_name = written.file_name().unwrap().to_string_lossy().to_string();
    assert!(
        content.contains(&format!("![截图.png](attachments/{sidecar_name})")),
        "got: {content:?}"
    );
}

/// §S2-4 未提供 attachments_dir：占位 attachment:// + warning。
#[tokio::test]
async fn dk09_enex_resource_without_dir_warns() {
    let app = test_app().await;
    let hash = "ffee0011223344556677889900aabbcc";
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export version="0.85">
  <note><title>无目录</title>
    <content><![CDATA[<en-note><en-media type="image/jpeg" hash="{hash}"></en-media></en-note>]]></content>
    <resource>
      <data encoding="base64" hash="{hash}">{b64}</data>
      <mime>image/jpeg</mime>
    </resource>
  </note>
</en-export>"#,
        b64 = b64(RESOURCE_BYTES)
    );
    let path = write_enex(app._dir.path(), "nodir.enex", &xml);
    let report = import_enex(&app.ctx, &path, &EnexImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 1);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("未落盘") && w.contains(hash)),
        "warnings: {:?}",
        report.warnings
    );
    let (_, content) = read_back(&app, &report.note_ids[0]).await;
    // 占位：alt = <hash>.jpeg（mime 推断），link = attachment://<hash>
    assert!(
        content.contains(&format!("![{hash}.jpeg](attachment://{hash})")),
        "got: {content:?}"
    );
}

/// §S2-5 ENML 结构映射：任务列表 / 代码块 / 表格。
#[tokio::test]
async fn dk09_enex_todo_codeblock_table() {
    let app = test_app().await;
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export version="0.85">
  <note><title>结构映射</title>
    <content><![CDATA[<en-note>
<div>计划：</div>
<en-todo checked="true"/>完成设计
<en-todo checked="false"/>联调测试
<en-codeblock lang="rust"><div>fn main() {}</div></en-codeblock>
<table><tr><th>项</th><th>状态</th></tr><tr><td>导入</td><td>进行中</td></tr></table>
</en-note>]]></content>
  </note>
</en-export>"#;
    let path = write_enex(app._dir.path(), "struct.enex", xml);
    let report = import_enex(&app.ctx, &path, &EnexImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 1);
    let (_, content) = read_back(&app, &report.note_ids[0]).await;

    assert!(content.contains("- [x] 完成设计"), "got: {content:?}");
    assert!(content.contains("- [ ] 联调测试"), "got: {content:?}");
    assert!(content.contains("```rust"), "got: {content:?}");
    assert!(content.contains("fn main() {}"), "got: {content:?}");
    assert!(content.contains("| 项 | 状态 |"), "got: {content:?}");
    assert!(content.contains("|---|---|"), "got: {content:?}");
    assert!(content.contains("| 导入 | 进行中 |"), "got: {content:?}");
}

/// §S2-6 en-media hash 无对应 resource：warning + attachment- 占位。
#[tokio::test]
async fn dk09_enex_hash_without_resource_warns() {
    let app = test_app().await;
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export version="0.85">
  <note><title>孤儿引用</title>
    <content><![CDATA[<en-note><en-media type="image/png" hash="deadbeefdeadbeefdeadbeefdeadbeef"></en-media></en-note>]]></content>
  </note>
</en-export>"#;
    let path = write_enex(app._dir.path(), "orphan.enex", xml);
    let report = import_enex(
        &app.ctx,
        &path,
        &EnexImportOptions {
            attachments_dir: Some(app._dir.path().join("att")),
        },
    )
    .await
    .expect("import ok");
    assert_eq!(report.imported, 1);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("deadbeef") && w.contains("无对应 resource")),
        "warnings: {:?}",
        report.warnings
    );
    let (_, content) = read_back(&app, &report.note_ids[0]).await;
    // 占位：alt = <hash 前 12 位>.bin（无 file-name/mime），link = attachment://<hash>
    assert!(
        content.contains("![deadbeefdead.bin](attachment://deadbeefdeadbeefdeadbeefdeadbeef)"),
        "got: {content:?}"
    );
}

/// §S2-7 损坏 XML：致命错误返回（不产生半成品导入）。
#[tokio::test]
async fn dk09_enex_invalid_xml_is_fatal_error() {
    let app = test_app().await;
    let path = write_enex(
        app._dir.path(),
        "bad.enex",
        "<en-export><note><title>未闭合",
    );
    let result = import_enex(&app.ctx, &path, &EnexImportOptions::default()).await;
    let err = result.expect_err("损坏 XML 必须报错");
    assert!(err.reason.contains("ENEX 解析失败"), "got: {err}");
    assert_eq!(err.path, path);
}

/// §S2-8 空标题兜底：`<title>` 缺失 → 未命名笔记 N。
#[tokio::test]
async fn dk09_enex_empty_title_fallback() {
    let app = test_app().await;
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export version="0.85">
  <note><content><![CDATA[<en-note><div>无标题</div></en-note>]]></content></note>
  <note><title>   </title><content><![CDATA[<en-note><div>空白标题</div></en-note>]]></content></note>
</en-export>"#;
    let path = write_enex(app._dir.path(), "notitle.enex", xml);
    let report = import_enex(&app.ctx, &path, &EnexImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 2);
    let titles: Vec<String> = report.entries.iter().map(|e| e.title.clone()).collect();
    assert!(titles.contains(&"未命名笔记 1".to_string()));
    assert!(titles.contains(&"未命名笔记 2".to_string()));
}
