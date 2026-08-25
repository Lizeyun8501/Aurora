//! V2 迁移测试 — notes 表 V19 §11 字段补齐（DEV-003）。

use super::super::*;

/// 新库: v1+v2 连续执行，notes 表含全部 V19 设计字段。
#[test]
fn v2_notes_columns_present() {
    let mgr = MigrationManager::new_in_memory().unwrap();
    mgr.migrate().unwrap();
    let conn = mgr.into_inner().unwrap();

    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(notes)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .flatten()
        .collect();

    for expected in [
        "file_path",
        "file_hash",
        "lamport_ts",
        "sync_state",
        "encryption",
        "is_deleted",
    ] {
        assert!(
            cols.iter().any(|c| c == expected),
            "notes 表缺少 V19 设计字段: {expected} (实际: {cols:?})"
        );
    }
}

/// 版本号: 全新库直达 v2。
#[test]
fn v2_schema_version_is_2() {
    let mgr = MigrationManager::new_in_memory().unwrap();
    mgr.migrate().unwrap();
    assert_eq!(CURRENT_SCHEMA_VERSION, 2);
}

/// 默认值: 新插入行自动获得 V19 字段默认值。
#[test]
fn v2_defaults_on_insert() {
    let mgr = MigrationManager::new_in_memory().unwrap();
    mgr.migrate().unwrap();
    let conn = mgr.into_inner().unwrap();

    conn.execute(
        "INSERT INTO notes (id, title, content, content_type, created_at, updated_at)
         VALUES ('n1', 't', '', 'markdown', '2026-01-01', '2026-01-01')",
        [],
    )
    .unwrap();

    let (lamport, sync_state, encryption, is_deleted, file_path): (i64, String, String, i64, String) =
        conn.query_row(
            "SELECT lamport_ts, sync_state, encryption, is_deleted, file_path FROM notes WHERE id='n1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();

    assert_eq!(lamport, 0);
    assert_eq!(sync_state, "synced");
    assert_eq!(encryption, "none");
    assert_eq!(is_deleted, 0);
    assert_eq!(file_path, "");
}

/// 旧库升级: v1 库中已删除笔记（deleted_at 非空）→ is_deleted 回填为 1。
#[test]
fn v2_backfill_is_deleted() {
    // 手工构造 v1 库（跳过 v2）
    let mgr = MigrationManager::new_in_memory().unwrap();
    {
        let mut conn = mgr.conn.lock().unwrap();
        // apply_v1 内部已记录 version=1
        MigrationManager::apply_v1_on(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO notes (id, title, content, content_type, created_at, updated_at, deleted_at)
             VALUES ('dead', 't', '', 'markdown', '2026-01-01', '2026-01-01', '2026-01-02')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO notes (id, title, content, content_type, created_at, updated_at)
             VALUES ('alive', 't', '', 'markdown', '2026-01-01', '2026-01-01')",
            [],
        )
        .unwrap();
    }
    // 跑完整迁移（应执行 v2）
    mgr.migrate().unwrap();
    let conn = mgr.into_inner().unwrap();

    let dead: i64 = conn
        .query_row("SELECT is_deleted FROM notes WHERE id='dead'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let alive: i64 = conn
        .query_row("SELECT is_deleted FROM notes WHERE id='alive'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(dead, 1, "deleted_at 非空的行应回填 is_deleted=1");
    assert_eq!(alive, 0);
}

/// 复合索引存在性: idx_notes_workspace 为 (workspace_id, updated_at DESC)。
#[test]
fn v2_composite_index() {
    let mgr = MigrationManager::new_in_memory().unwrap();
    mgr.migrate().unwrap();
    let conn = mgr.into_inner().unwrap();

    let idx: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='index' AND name='idx_notes_workspace'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let sql = idx.expect("复合索引应存在");
    assert!(sql.contains("updated_at DESC"), "索引应为复合: {sql}");
}
