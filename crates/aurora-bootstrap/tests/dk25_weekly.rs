//! DK-25 周回顾全周口径 — 行为级测试。
//!
//! DoD：周边界含周一 00:00 / 周日 23:59:59.999、跨月、空周返回空摘要。
//! 数据注入走 TaskSource 回调（构造 TaskViewRow 字面量含 completed_at），
//! 完成时刻 stamp 链（TaskStatusChanged → 终态 stamp / 非终态清 None）
//! 由 aurora-core 内联测试覆盖。

use aurora_core::l1_infrastructure::storage_engine::MemoryKVStore;
use aurora_core::l2_engines::task_projection::{TaskProjection, TaskViewRow};
use std::sync::Arc;

fn row(id: &str, completed_at: Option<i64>) -> TaskViewRow {
    TaskViewRow {
        task_id: id.into(),
        note_id: "n1".into(),
        block_id: String::new(),
        title: format!("任务{id}"),
        status: if completed_at.is_some() {
            "done"
        } else {
            "next"
        }
        .into(),
        priority: "medium".into(),
        due_date: None,
        estimate_minutes: 60,
        actual_minutes: 90,
        parent_task_id: String::new(),
        progress: 0.0,
        completed_at,
    }
}

async fn proj(rows: Vec<TaskViewRow>) -> TaskProjection {
    use aurora_core::event_bus::projection::Projection;
    let kv = Arc::new(MemoryKVStore::default());
    let p = TaskProjection::new(kv, Box::new(move || rows.clone()));
    p.rebuild().await.expect("source 灌入 rows");
    p
}

/// 周锚点：2026-01-05 为周一。区间 [周一 00:00.000, 次周一 00:00.000 - 1ms]。
const MON_0000: i64 = 1767571200000; // 2026-01-05T00:00:00Z
const SUN_LAST_MS: i64 = 1768175999999; // 2026-01-11T23:59:59.999Z
const NEXT_MON_0000: i64 = 1768176000000; // 2026-01-12T00:00:00Z

#[tokio::test]
async fn dk25_week_boundary_inclusive() {
    let p = proj(vec![
        row("at-mon-0000", Some(MON_0000)),
        row("at-sun-last-ms", Some(SUN_LAST_MS)),
        row("at-next-mon", Some(NEXT_MON_0000)),
        row("uncompleted", None),
    ])
    .await;
    let ids = p.completed_task_ids_between(MON_0000, SUN_LAST_MS);
    assert!(
        ids.contains(&"at-mon-0000".to_string()),
        "周一 00:00:00.000 必须含（左闭）"
    );
    assert!(
        ids.contains(&"at-sun-last-ms".to_string()),
        "周日 23:59:59.999 必须含（右闭）"
    );
    assert!(
        !ids.contains(&"at-next-mon".to_string()),
        "下周一 00:00:00.000 不得含（区间右端开于次周）"
    );
    assert_eq!(ids.len(), 2);
}

#[tokio::test]
async fn dk25_cross_month_week() {
    // 2026-01-26（周一）~ 2026-02-01（周日）：同一 ISO 周跨 1 月与 2 月
    let jan29 = 1769673600000; // 2026-01-29T08:00Z（周四）
    let feb1 = 1769911200000; // 2026-02-01T02:00Z（周日）
    let feb2 = 1769990400000; // 2026-02-02T00:00Z（次周一，界外）
    let p = proj(vec![
        row("jan-side", Some(jan29)),
        row("feb-side", Some(feb1)),
        row("feb-next-week", Some(feb2)),
    ])
    .await;
    let ids = p.completed_task_ids_between(1769385600000, 1769990399999);
    assert!(
        ids.contains(&"jan-side".to_string()) && ids.contains(&"feb-side".to_string()),
        "跨月周两侧都必须命中（毫秒区间天然跨月）"
    );
    assert!(!ids.contains(&"feb-next-week".to_string()));
}

#[tokio::test]
async fn dk25_empty_week_returns_empty_summary() {
    let p = proj(vec![row("no-completed", None)]).await;
    let ids = p.completed_task_ids_between(MON_0000, SUN_LAST_MS);
    assert!(ids.is_empty(), "空周 → 空 Vec");
    let s = p.weekly_summary(&ids);
    assert_eq!(s.total_estimate_minutes, 0);
    assert_eq!(s.total_actual_minutes, 0);
    assert!(s.deviation_rate.is_none(), "空集无预估 → None（除零防护）");
    assert!(s.per_task.is_empty());
}

/// stamp 链：TaskStatusChanged 转终态（done/cancelled）stamp、转回非终态清 None。
#[tokio::test]
async fn dk25_completed_at_stamp_chain() {
    use aurora_core::event_bus::layered::AppEvent;
    use aurora_core::event_bus::projection::Projection;
    let kv = Arc::new(MemoryKVStore::default());
    let p = TaskProjection::new(kv, Box::new(Vec::<TaskViewRow>::new));
    p.seed_row("t-a", "n1", "写稿", "next", "high", None);

    p.apply(&AppEvent::TaskStatusChanged {
        task_id: "t-a".into(),
        old_status: "next".into(),
        new_status: "done".into(),
    })
    .await
    .unwrap();
    let ids = p.completed_task_ids_between(0, i64::MAX);
    assert!(ids.contains(&"t-a".to_string()), "done → stamp 完成时刻");

    p.apply(&AppEvent::TaskStatusChanged {
        task_id: "t-a".into(),
        old_status: "done".into(),
        new_status: "next".into(),
    })
    .await
    .unwrap();
    assert!(
        p.completed_task_ids_between(0, i64::MAX).is_empty(),
        "回退非终态 → completed_at 清 None"
    );
}
