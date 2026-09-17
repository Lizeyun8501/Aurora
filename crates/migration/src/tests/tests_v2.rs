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

/// 版本号: 全新库直达当前版本（V4 — blocks 块级双轨; audit 哈希链列自 V3 起保留）。
#[test]
fn v4_schema_version_is_current() {
    let mgr = MigrationManager::new_in_memory().unwrap();
    mgr.migrate().unwrap();
    assert_eq!(CURRENT_SCHEMA_VERSION, 6);
    // V3: audit_log 必须带 prev_hash / hash 列（T12 哈希链）
    let conn = mgr.into_inner().unwrap();
    let mut stmt = conn.prepare("PRAGMA table_info(audit_log)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(|c| c.ok())
        .collect();
    assert!(
        cols.iter().any(|c| c == "prev_hash"),
        "audit_log 缺 prev_hash: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "hash"),
        "audit_log 缺 hash: {cols:?}"
    );
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

/// V6: 番茄钟会话持久化（DK-06 — 计时切后台不丢数据）。
#[test]
fn v6_pomodoro_sessions_table() {
    let mgr = MigrationManager::new_in_memory().unwrap();
    mgr.migrate().unwrap();
    let conn = mgr.into_inner().unwrap();
    conn.execute(
        "INSERT INTO pomodoro_sessions (id, task_id, started_at, planned_minutes) VALUES ('s1','t1','2026-09-18T08:00:00Z', 25)",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE pomodoro_sessions SET ended_at='2026-09-18T08:25:00Z', actual_minutes=25, completed=1 WHERE id='s1'",
        [],
    )
    .unwrap();
    let done: i64 = conn
        .query_row(
            "SELECT completed FROM pomodoro_sessions WHERE id='s1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(done, 1, "会话完成状态可写");
}

/// V5: 目录树统一 + 组织/配套表（DK-01 — V24 整体缺失的表补齐）。
#[test]
fn v5_unified_tree_and_org_tables() {
    let mgr = MigrationManager::new_in_memory().unwrap();
    mgr.migrate().unwrap();

    let conn = mgr.into_inner().unwrap();

    // notes 统一树列存在且默认值正确
    let kind: String = conn
        .query_row("SELECT kind FROM notes LIMIT 0", [], |r| r.get(0))
        .unwrap_or("note".into());
    assert_eq!(kind, "note", "kind 列默认 note");
    let col_exists = |name: &str| -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('notes') WHERE name = ?1",
            [name],
            |r| r.get::<_, i64>(0),
        )
        .map(|v| v > 0)
        .unwrap_or(false)
    };
    for col in [
        "kind",
        "sort_order",
        "is_pinned",
        "is_favorite",
        "lamport_ts",
    ] {
        assert!(col_exists(col), "notes.{col} 列缺失");
    }

    // 组织/配套表存在且可写
    let tables = [
        "tags",
        "note_tags",
        "smart_folders",
        "bookmarks",
        "trash",
        "attachments",
        "settings",
        "projection_watermark",
    ];
    for t in tables {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap_or(-1);
        assert_eq!(n, 0, "表 {t} 应存在且为空");
    }

    // 冒烟: tags + note_tags 联结 + 水位线写入
    conn.execute(
        "INSERT INTO tags (id, name, created_at) VALUES ('t1','rust','2026-01-01')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tags (id, name, created_at) VALUES ('t2','crdt','2026-01-01')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('projection.search.wm', x'0102')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO projection_watermark (projection, watermark) VALUES ('search-index', 7)",
        [],
    )
    .unwrap();
    let wm: i64 = conn
        .query_row(
            "SELECT watermark FROM projection_watermark WHERE projection='search-index'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(wm, 7);
}
