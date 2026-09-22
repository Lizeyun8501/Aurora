//! DK-09 收官切片：导入器附件接管（任务书 bravo-DK09-import-attachments）。
//!
//! 验收矩阵（§5）：
//! - enex 2 资源 → list_by_note 2 条 meta + 正文 2 处 `attachment://`；
//! - 去重：同资源挂两笔记 → blob 键只存一份（scan_prefix 计数）；
//! - read_attachment 回读一致（明文 + seal 两模式）；
//! - attach 失败 fail-closed：笔记计入 failed，无残留；
//! - markdown 缺失资源 → attachments_missing 计数且导入不失败。

use aurora_core::write_path::{read_attachment, WriteContext};
use aurora_import::{import_enex, import_markdown_dir, EnexImportOptions, ImportOptions};
use std::path::{Path, PathBuf};

struct TestApp {
    _dir: tempfile::TempDir,
    ctx: WriteContext,
}

fn test_app() -> TestApp {
    let dir = tempfile::tempdir().expect("tempdir");
    let booted = aurora_bootstrap::bootstrap(dir.path()).expect("bootstrap");
    TestApp {
        _dir: dir,
        ctx: WriteContext {
            core: booted.core,
            blocks: booted.blocks,
            seal: None,
            // 附件模式（与桌面同源；seal None = 明文降级语义）
            attachments: Some(booted.attachments),
            content_cipher: None,
        },
    }
}

/// seal 模式 app：bootstrap vault 构造 SealPair（桌面语义）。
fn test_app_sealed() -> TestApp {
    let dir = tempfile::tempdir().expect("tempdir");
    let booted = aurora_bootstrap::bootstrap(dir.path()).expect("bootstrap");
    let crypto = booted.core.crypto.clone();
    let crypto2 = crypto.clone();
    let vault = booted.vault.clone();
    let vault2 = vault.clone();
    let seal = aurora_core::write_path::SealPair {
        seal: Box::new(move |b: &[u8]| {
            vault
                .encrypt(crypto.as_ref(), b)
                .map_err(|e| aurora_core::Error::Internal(e.to_string()))
        }),
        unseal: Box::new(move |b: &[u8]| {
            vault2
                .decrypt(crypto2.as_ref(), b)
                .map_err(|e| aurora_core::Error::Internal(e.to_string()))
        }),
    };
    TestApp {
        _dir: dir,
        ctx: WriteContext {
            core: booted.core,
            blocks: None,
            seal: Some(seal),
            attachments: Some(booted.attachments),
            content_cipher: None,
        },
    }
}

fn write_bytes(root: &Path, rel: &str, data: &[u8]) -> PathBuf {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&path, data).expect("write");
    path
}

/// 1×1 PNG（43 字节，合法文件头，满足 fixtures <10KB 约束）。
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

const TINY_TXT: &[u8] = b"aurora-attachment-payload";

/// fixtures：<10KB 静态资源（任务书 §4）。
fn fixture_png() -> Vec<u8> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.png");
    std::fs::read(&p).unwrap_or_else(|_| TINY_PNG.to_vec())
}

async fn read_body(app: &TestApp, note_id: &str) -> String {
    let record = aurora_core::write_path::load_note_meta(
        app.ctx.core.as_ref(),
        note_id,
        app.ctx.seal.as_ref(),
    )
    .await
    .expect("load_note_meta")
    .expect("note exists");
    aurora_core::write_path::open_note_content(&app.ctx, note_id, &record).expect("open content")
}

/// §5-2：enex 2 资源笔记 → list_by_note 2 条 + 正文 2 处 attachment:// 引用。
#[tokio::test]
async fn dk09_attach_enex_two_resources() {
    let app = test_app();
    let b64 = |data: &[u8]| {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(data)
    };
    let png = fixture_png();
    let enex = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export application="Evernote">
  <note><title>双资源</title><content><![CDATA[<?xml version="1.0"?><en-note><p>图:</p><en-media type="image/png" hash="aa11"/><p>文:</p><en-media type="text/plain" hash="bb22"/></en-note>]]></content><resource><mime>image/png</mime><data encoding="base64" hash="aa11">{p}</data></resource><resource><mime>text/plain</mime><data encoding="base64" hash="bb22">{t}</data></resource></note>
</en-export>"#,
        p = b64(&png),
        t = b64(TINY_TXT),
    );
    let path = write_bytes(app._dir.path(), "two.enex", enex.as_bytes());
    let report = import_enex(&app.ctx, &path, &EnexImportOptions::default())
        .await
        .expect("r1");
    assert_eq!(report.imported, 1, "{:?}", report.errors);
    assert_eq!(report.attachments_imported, 2);
    assert_eq!(report.entries[0].resources.len(), 2);

    // list_by_note 2 条 meta（行为级：真实存储层）
    let metas = app
        .ctx
        .attachments
        .as_ref()
        .expect("attach")
        .list_by_note(&report.note_ids[0])
        .await
        .expect("list_by_note");
    assert_eq!(metas.len(), 2);

    // 正文 2 处 attachment:// 引用 + hash→id 重写正确（两 id 互异且都在 meta 里）
    let body = read_body(&app, &report.note_ids[0]).await;
    assert_eq!(body.matches("attachment://").count(), 2, "{body}");
    let ids: Vec<&str> = report.entries[0]
        .resources
        .iter()
        .filter_map(|r| r.attachment_id.as_deref())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    for id in &ids {
        assert!(body.contains(&format!("attachment://{id}")), "{body}");
    }
}

/// §5-3：同资源挂两笔记 → blob 只存一份（scan_prefix blob 键计数）。
#[tokio::test]
async fn dk09_attach_dedup_blob_single_copy() {
    use aurora_core::attachment_store::blob_key;
    use base64::Engine as _;

    let app = test_app();
    let b64 = base64::engine::general_purpose::STANDARD.encode(TINY_PNG);
    let enex = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export application="Evernote">
  <note><title>甲</title><content><![CDATA[<?xml version="1.0"?><en-note><en-media type="image/png" hash="cc"/></en-note>]]></content><resource><mime>image/png</mime><data encoding="base64" hash="cc">{b64}</data></resource></note>
  <note><title>乙</title><content><![CDATA[<?xml version="1.0"?><en-note><en-media type="image/png" hash="cc"/></en-note>]]></content><resource><mime>image/png</mime><data encoding="base64" hash="cc">{b64}</data></resource></note>
</en-export>"#
    );
    let path = write_bytes(app._dir.path(), "dup.enex", enex.as_bytes());
    let report = import_enex(&app.ctx, &path, &EnexImportOptions::default())
        .await
        .expect("r");
    assert_eq!(report.imported, 2);
    assert_eq!(report.attachments_imported, 2, "各挂一次均计数");

    // blob 前缀扫描：同 sha256 只占一个 blob 键
    let kv = app.ctx.core.kv_store.clone();
    let sha_hex = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(TINY_PNG);
        hex(&h.finalize())
    };
    let blobs = kv.scan_prefix(&blob_key_prefix()).await.expect("scan");
    assert_eq!(blobs.len(), 1, "blob 应只存一份");
    assert_eq!(blobs[0].0, blob_key(&sha_hex));
}

fn blob_key_prefix() -> String {
    use aurora_core::attachment_store::blob_key;
    // attachment_store 内部常量（att/blob/）——经 pub fn blob_key 前缀推导
    let probe = blob_key(&"0".repeat(64));
    probe[..probe.len() - 64].to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// §5-4：read_attachment 回读字节一致（明文 + seal 两模式）。
#[tokio::test]
async fn dk09_attach_readback_plaintext_and_sealed() {
    let b64 = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(TINY_TXT)
    };
    let enex = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export application="Evernote">
  <note><title>回读</title><content><![CDATA[<?xml version="1.0"?><en-note><en-media type="text/plain" hash="dd"/></en-note>]]></content><resource><mime>text/plain</mime><data encoding="base64" hash="dd">{b64}</data></resource></note>
</en-export>"#
    );

    for (label, app) in [("明文", test_app()), ("seal", test_app_sealed())] {
        let path = write_bytes(app._dir.path(), "rb.enex", enex.as_bytes());
        let report = import_enex(&app.ctx, &path, &EnexImportOptions::default())
            .await
            .expect(label);
        assert_eq!(report.imported, 1, "{label}");
        let id = report.entries[0].resources[0]
            .attachment_id
            .as_ref()
            .expect(label)
            .clone();
        let (meta, data) = read_attachment(&app.ctx, &id).await.expect(label);
        assert_eq!(data, TINY_TXT, "{label} 回读一致");
        assert_eq!(meta.size, TINY_TXT.len() as u64, "{label} 明文尺寸");
    }
}

/// §5-fail-closed：attach 失败 → 笔记计 failed 且无残留（delete_note 级联）。
#[tokio::test]
async fn dk09_attach_fail_closed_no_residual() {
    use async_trait::async_trait;
    use aurora_core::attachment_store::{AttachmentMeta, AttachmentStore};
    use aurora_core::Error;

    struct BrokenStore;

    #[async_trait]
    impl AttachmentStore for BrokenStore {
        async fn put(&self, _meta: &AttachmentMeta, _sealed: &[u8]) -> Result<(), Error> {
            Err(Error::Internal("broken store".into()))
        }
        async fn get_meta(&self, _id: &str) -> Result<Option<AttachmentMeta>, Error> {
            Ok(None)
        }
        async fn get_blob(&self, _sha: &str) -> Result<Option<Vec<u8>>, Error> {
            Ok(None)
        }
        async fn list_by_note(&self, _note_id: &str) -> Result<Vec<AttachmentMeta>, Error> {
            Ok(Vec::new())
        }
        async fn delete(&self, _attachment_id: &str) -> Result<(), Error> {
            Ok(())
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let booted = aurora_bootstrap::bootstrap(dir.path()).expect("bootstrap");
    let app = TestApp {
        _dir: dir,
        ctx: WriteContext {
            core: booted.core,
            blocks: None,
            seal: None,
            attachments: Some(std::sync::Arc::new(BrokenStore)),
            content_cipher: None,
        },
    };

    let b64 = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(TINY_TXT)
    };
    let enex = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<en-export application="Evernote">
  <note><title>会失败</title><content><![CDATA[<?xml version="1.0"?><en-note><en-media type="text/plain" hash="ee"/></en-note>]]></content><resource><mime>text/plain</mime><data encoding="base64" hash="ee">{b64}</data></resource></note>
</en-export>"#
    );
    let path = write_bytes(app._dir.path(), "bad.enex", enex.as_bytes());
    let report = import_enex(&app.ctx, &path, &EnexImportOptions::default())
        .await
        .expect("r");
    assert_eq!(report.imported, 0);
    assert_eq!(report.failed, 1);
    assert_eq!(report.attachments_imported, 0);
    assert!(
        report.errors[0].reason.contains("attach 失败"),
        "{:?}",
        report.errors[0].reason
    );
    // 无残留：note: 元数据键为空（delete_note 级联清理；notesnap: 不匹配前缀）
    let residual = app
        .ctx
        .core
        .kv_store
        .scan_prefix("note:")
        .await
        .expect("scan");
    assert!(
        residual.is_empty(),
        "fail-closed 不留半截笔记: {:?}",
        residual
    );
}

/// §5-5：markdown 缺失资源 → attachments_missing 计数且导入不失败。
#[tokio::test]
async fn dk09_attach_md_missing_resource_tolerant() {
    let app = test_app();
    let root = app._dir.path();
    write_bytes(root, "notes/ok.md", "# 有图\n\n![x](tiny.png)\n".as_bytes());
    write_bytes(root, "notes/tiny.png", TINY_PNG);
    write_bytes(
        root,
        "notes/missing.md",
        "# 缺图\n\n![y](ghost.png)\n".as_bytes(),
    );

    let report = import_markdown_dir(&app.ctx, &root.join("notes"), &ImportOptions::default())
        .await
        .expect("r");
    assert_eq!(report.imported, 2, "{:?}", report.errors);
    assert_eq!(report.attachments_imported, 1, "tiny.png 挂接");
    assert_eq!(report.attachments_missing, 1, "ghost.png 缺失计数");

    // 遍历按文件名排序：missing.md 在前，ok.md 在后
    let miss_body = read_body(&app, &report.note_ids[0]).await;
    assert!(
        miss_body.contains("ghost.png"),
        "缺失链接原样保留: {miss_body}"
    );
    assert!(
        !miss_body.contains("attachment://"),
        "缺失不产生引用: {miss_body}"
    );

    let ok_body = read_body(&app, &report.note_ids[1]).await;
    assert!(
        ok_body.contains("attachment://"),
        "相对路径已被重写: {ok_body}"
    );
    assert!(
        !ok_body.contains("tiny.png"),
        "原始相对路径不残留: {ok_body}"
    );
}

/// 同 markdown 内同一路径复用同一 attachment id（cache 语义）。
#[tokio::test]
async fn dk09_attach_md_cache_reuse() {
    let app = test_app();
    let root = app._dir.path();
    let md = "# 图 twice\n\n![a](tiny.png)\n\n![b](tiny.png)\n".as_bytes();
    write_bytes(root, "mds/twice.md", md);
    write_bytes(root, "mds/tiny.png", TINY_PNG);

    let report = import_markdown_dir(&app.ctx, &root.join("mds"), &ImportOptions::default())
        .await
        .expect("r");
    assert_eq!(report.imported, 1);
    assert_eq!(
        report.attachments_imported, 1,
        "同路径复用 id，只 attach 一次"
    );
    let body = read_body(&app, &report.note_ids[0]).await;
    let ids: Vec<_> = body
        .split("attachment://")
        .skip(1)
        .map(|s| s.split(')').next().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], ids[1], "两引用同 id");
}
