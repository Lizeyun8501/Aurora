//! DK-09 第一切片集成测试 — Markdown 目录导入。
//!
//! 真实装配：`aurora_bootstrap::bootstrap(tempdir)` 提供与桌面端同源的
//! AppCore（SQLite KV + blocks 派生 + 启动流程），写入走 write_path 唯一
//! 入口；断言用 `load_note_meta` 读回 `NoteRecord`（`get_note_content`
//! 路径在 core 侧的权威等价物：`record.content` 即 `get_note_content`
//! 读出的正文来源，加密级 none 时为明文）。
//!
//! S5 教训（任务书 §8-4）：每个 `dk09_*` 测试名在交付前用
//! `cargo test -p aurora-import 2>&1 | grep "test dk09"` 逐一确认真实执行。

use std::path::Path;

use aurora_core::write_path::{load_note_meta, WriteContext};
use aurora_import::{import_markdown_dir, ImportOptions};

/// 真实装配的测试上下文（tempdir 随测试结束清理）。
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
            seal: None,           // 明文落盘（与移动端语义一致）
            content_cipher: None, // 无内容级加密
        },
    }
}

/// 读回笔记权威内容（get_note_content 路径等价物）。
async fn read_back(app: &TestApp, note_id: &str) -> (String, String) {
    let record = load_note_meta(app.ctx.core.as_ref(), note_id, None)
        .await
        .expect("load_note_meta")
        .expect("note exists");
    (record.title, record.content)
}

fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

/// §5.3-1 基础导入：根目录两个 .md → 两篇笔记，标题 = 文件名 stem，
/// 正文读回逐字符一致（含代码块）。
#[tokio::test]
async fn dk09_import_markdown_dir_basic() {
    let app = test_app().await;
    let src = app._dir.path().join("src");
    write_file(
        &src,
        "读书笔记.md",
        "# 读书笔记\n\n```rust\nfn main() {}\n```\n",
    );
    write_file(&src, "周计划.md", "## 周一\n- [ ] 站会\n");

    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.scanned, 2);
    assert_eq!(report.imported, 2);
    assert_eq!(report.failed, 0);
    assert_eq!(report.note_ids.len(), 2);

    // walkdir 按文件名字节序遍历（周 E5 < 读 E8），断言不依赖顺序
    let mut got = Vec::new();
    for id in &report.note_ids {
        got.push(read_back(&app, id).await);
    }
    got.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(got[0].0, "周计划");
    assert_eq!(got[0].1, "## 周一\n- [ ] 站会\n");
    assert_eq!(got[1].0, "读书笔记");
    assert_eq!(
        got[1].1, "# 读书笔记\n\n```rust\nfn main() {}\n```\n",
        "正文必须逐字符无损"
    );
}

/// §5.3-2 嵌套子目录：多层子目录全部递归导入。
#[tokio::test]
async fn dk09_import_nested_subdirectories() {
    let app = test_app().await;
    let src = app._dir.path().join("vault");
    write_file(&src, "root.md", "root\n");
    write_file(&src, "projects/aurora/plan.md", "plan\n");
    write_file(&src, "projects/aurora/deep/diary.md", "diary\n");

    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 3, "嵌套文件必须全部导入");
    assert_eq!(report.note_ids.len(), 3);

    let mut titles = Vec::new();
    for id in &report.note_ids {
        let (t, _) = read_back(&app, id).await;
        titles.push(t);
    }
    titles.sort();
    assert_eq!(titles, vec!["diary", "plan", "root"]);
}

/// §5.3-3 标题规则：frontmatter 合法 `title:` 优先；无 frontmatter 用 stem。
#[tokio::test]
async fn dk09_import_title_from_frontmatter() {
    let app = test_app().await;
    let src = app._dir.path().join("notes");
    write_file(&src, "file_one.md", "---\ntitle: 优雅的标题\n---\n正文\n");
    write_file(&src, "file_two.md", "无 frontmatter\n");

    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 2);

    let (t0, _) = read_back(&app, &report.note_ids[0]).await;
    assert_eq!(t0, "优雅的标题", "frontmatter title 必须优先");
    let (t1, _) = read_back(&app, &report.note_ids[1]).await;
    assert_eq!(t1, "file_two", "无 frontmatter 用文件名 stem");
}

/// §5.3-4 frontmatter 剥离：不进正文（避免渲染出 `---` 横线），其余原样。
#[tokio::test]
async fn dk09_import_frontmatter_stripped() {
    let app = test_app().await;
    let src = app._dir.path().join("notes");
    let body_after = "# 正文\n\ntext\n";
    write_file(
        &src,
        "with_fm.md",
        "---\ntitle: 标题\ntags: [a, b]\n---\n# 正文\n\ntext\n",
    );

    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 1);
    let (_, content) = read_back(&app, &report.note_ids[0]).await;
    assert_eq!(content, body_after, "frontmatter 必须剥离");
    assert!(!content.contains("title:"), "title 字段不得残留正文");
    assert!(!content.contains("---"), "横线不得残留正文");
}

/// §5.3-4 警告路径：frontmatter 无 title / 未闭合 → stem + 原样导入。
#[tokio::test]
async fn dk09_import_frontmatter_fallback_warns() {
    let app = test_app().await;
    let src = app._dir.path().join("notes");
    write_file(&src, "no_title.md", "---\ntags: [x]\n---\n内容A\n");
    write_file(&src, "unclosed.md", "---\ntitle: 未闭合\n内容B\n");

    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 2);
    assert_eq!(report.warnings.len(), 2, "两条警告都要体现");

    let (t0, c0) = read_back(&app, &report.note_ids[0]).await;
    assert_eq!(t0, "no_title", "无 title 字段 → 文件名 stem");
    assert_eq!(c0, "内容A\n", "frontmatter 仍被剥离");
    let (t1, c1) = read_back(&app, &report.note_ids[1]).await;
    assert_eq!(t1, "unclosed");
    assert_eq!(
        c1, "---\ntitle: 未闭合\n内容B\n",
        "未闭合 frontmatter 原样无损"
    );
}

/// §5.3-5 跳过规则：非 .md 与隐藏文件/目录跳过，计数进 report.skipped。
#[tokio::test]
async fn dk09_import_skip_non_markdown_and_hidden() {
    let app = test_app().await;
    let src = app._dir.path().join("vault");
    write_file(&src, "keep.md", "md\n");
    write_file(&src, "ignore.txt", "txt\n");
    write_file(&src, "image.png", "png\n");
    write_file(&src, ".obsidian/config.md", "config\n");
    write_file(&src, ".hidden.md", "hidden\n");

    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 1);
    assert_eq!(report.scanned, 1);
    assert_eq!(report.skipped, 4, "txt/png/隐藏目录内 md/隐藏 md 全跳过");
    let (_, content) = read_back(&app, &report.note_ids[0]).await;
    assert_eq!(content, "md\n");
}

/// §5.3-6 内容无损底线：代码块（含 ``` 围栏与语言标注）、表格、任务
/// 列表、行尾风格逐字符读回对比 — 导入零格式损失。
#[tokio::test]
async fn dk09_import_no_loss_complex_markdown() {
    let app = test_app().await;
    let src = app._dir.path().join("complex");
    let body = "# 复杂结构\n\n\
        ```rust 设围栏语言也保留\n\
        let s = \"---\\n不是 frontmatter\";\n\
        ```\n\n\
        | 列A | 列B |\n\
        |-----|-----|\n\
        |  1  |  2  |\n\n\
        - [x] 已完成任务\n\
        - [ ] 待办任务\n\
        \n\
        > 引用块与*强调*、**粗体**\n";
    write_file(&src, "复杂.md", body);

    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.imported, 1);
    let (_, content) = read_back(&app, &report.note_ids[0]).await;
    assert_eq!(content, body, "代码块/表格/任务列表/引用必须逐字符无损");
}

/// §5.3-7 报告语义：计数守恒（scanned = imported + failed）、错误明细、
/// 空目录零导入不报错。
#[tokio::test]
async fn dk09_import_report_counts_and_empty_dir() {
    let app = test_app().await;
    let empty = app._dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let report = import_markdown_dir(&app.ctx, &empty, &ImportOptions::default())
        .await
        .expect("空目录应成功");
    assert_eq!(report.scanned, 0);
    assert_eq!(report.imported, 0);
    assert!(report.warnings.is_empty());
    assert!(!report.has_failures());
    assert!(report.duration_ms < 60_000);

    // 无效 UTF-8 的 .md → failed + errors 明细，不阻断其他文件
    let src = app._dir.path().join("mixed");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("bad.md"), [0xff, 0xfe, 0x00, 0x01]).unwrap();
    std::fs::write(src.join("good.md"), "ok\n").unwrap();
    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import ok");
    assert_eq!(report.scanned, 2);
    assert_eq!(report.imported, 1);
    assert_eq!(report.failed, 1);
    assert_eq!(report.scanned, report.imported + report.failed, "守恒");
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].reason.contains("读取失败"));
}

/// §S3 铺路：ImportReport JSON 序列化往返（桌面 command 返回载荷）。
#[tokio::test]
async fn dk09_import_report_json_roundtrip() {
    let app = test_app().await;
    let src = app._dir.path().join("r");
    write_file(&src, "a.md", "hello\n");
    let report = import_markdown_dir(&app.ctx, &src, &ImportOptions::default())
        .await
        .expect("import ok");
    let json = serde_json::to_string(&report).expect("serialize");
    let back: aurora_import::report::ImportReport =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.imported, 1);
    assert_eq!(back.entries[0].title, "a");
    assert_eq!(back.entries[0].note_id, report.entries[0].note_id);
}
