//! A-08: 独立任务域（日历消费面）——移动端 GTD Task 的 core 权威镜像。
//!
//! 与 `entity::Task`（笔记块内联任务,v19）**不是同一域**: CalendarTask 值域
//! 对齐移动端 GTD（status=next/waiting/plan/done 透传;due=「YYYY-MM-DD」）。
//!
//! 存储模式同 `event_store`（KV 直存 JSON,`cal-task:{id}`;scan_prefix 列全
//! +内存过滤——量级:个人任务 O(10^3)）。ADR-003 日历域例外同裁决
//! （2026-10-29: 无 seal/attachments 依赖,`&dyn KVStore` 直注）。
//!
//! 写路径纪律（A-08 铁律）:
//! - 全量镜像: `sync_tasks`（移动端双写面——增删改一次覆盖,差集删除）
//! - 改期专项: `reschedule_task` → **发 `AppEvent::TaskRescheduled`**（投影
//!   与另一端日历 1 个同步周期可见——DoD）
//! - fail-closed: 幽灵改期/空文本 → Domain 错误,禁止静默假装成功

use serde::{Deserialize, Serialize};

use crate::error_codes::ErrorCode;
use crate::event_bus::layered::AppEvent;
use crate::event_bus::layered::LayeredEventBus;
use crate::traits::kv_store::KVStore;
use crate::Error;

const TASK_PREFIX: &str = "cal-task:";

fn task_key(id: &str) -> String {
    format!("{TASK_PREFIX}{id}")
}

fn domain_err(code: ErrorCode) -> Error {
    Error::Domain { code, message: "" }
}

/// 日历任务实体（移动端 GTD Task 全字段镜像——`ext` 透传扩展字段无损往返）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalendarTask {
    pub id: String,
    pub text: String,
    /// GTD 状态（next/waiting/plan/done——string 透传,core 不枚举化避免值域漂移）。
    pub status: String,
    /// 截止日「YYYY-MM-DD」（None=未排期）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
    /// 创建时刻（epoch ms——与移动端 createdAt 同构）。
    pub created_at: i64,
    /// 优先级（high/normal/low）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// 移动端扩展字段透传（estimate/actual/parent_task_id/progress 等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

impl CalendarTask {
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            status: "next".into(),
            due: None,
            note_id: None,
            created_at: 0,
            priority: None,
            ext: None,
        }
    }
}

/// 校验（fail-closed）: 空文本/幽灵 id 拒收。
fn validate(t: &CalendarTask) -> Result<(), Error> {
    if t.id.trim().is_empty() {
        return Err(domain_err(ErrorCode::A04));
    }
    if t.text.trim().is_empty() {
        return Err(domain_err(ErrorCode::A04));
    }
    Ok(())
}

/// 全量镜像同步（移动端双写面）: 传入即权威全集——差集删除（不在清单内的
/// `cal-task:` 键清除）。返回写入/删除计数。
pub async fn sync_tasks(kv: &dyn KVStore, tasks: &[CalendarTask]) -> Result<(usize, usize), Error> {
    for t in tasks {
        validate(t)?;
    }
    let mut existing: Vec<String> = Vec::new();
    for (key, raw) in kv.scan_prefix(TASK_PREFIX).await? {
        match serde_json::from_slice::<CalendarTask>(&raw) {
            Ok(t) => existing.push(t.id),
            Err(e) => tracing::warn!(key = %key, error = %e, "cal-task record skipped"),
        }
    }
    let incoming: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    let mut deleted = 0usize;
    for id in existing {
        if !incoming.contains(&id.as_str()) {
            kv.delete(&task_key(&id)).await?;
            deleted += 1;
        }
    }
    let mut written = 0usize;
    for t in tasks {
        kv.set(&task_key(&t.id), &serde_json::to_vec(t)?).await?;
        written += 1;
    }
    Ok((written, deleted))
}

/// 全量列表（serde 失败 warn 跳过——同 event_store list 惯例）。
pub async fn list_tasks(kv: &dyn KVStore) -> Result<Vec<CalendarTask>, Error> {
    let mut out: Vec<CalendarTask> = Vec::new();
    for (key, raw) in kv.scan_prefix(TASK_PREFIX).await? {
        match serde_json::from_slice::<CalendarTask>(&raw) {
            Ok(t) => out.push(t),
            Err(e) => tracing::warn!(key = %key, error = %e, "cal-task record skipped"),
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// 改期（桌面拖拽/双端改期入口）→ 发 `TaskRescheduled`（Medium 持久化——
/// 投影与另一端日历同步）。幽灵任务 fail-closed（A06 惯例同 event_store 幽灵更新）。
pub async fn reschedule_task(
    kv: &dyn KVStore,
    bus: &LayeredEventBus,
    task_id: &str,
    new_due: Option<String>,
) -> Result<(), Error> {
    let raw = kv
        .get(&task_key(task_id))
        .await?
        .ok_or_else(|| domain_err(ErrorCode::A06))?;
    let mut t: CalendarTask = serde_json::from_slice(&raw)?;
    let old_due = t.due.clone();
    t.due = new_due.clone();
    kv.set(&task_key(task_id), &serde_json::to_vec(&t)?).await?;
    bus.publish(AppEvent::TaskRescheduled {
        task_id: task_id.into(),
        old_due,
        new_due,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::layered::EventQueueStore;
    use crate::event_bus::layered::InMemoryEventQueue;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use std::sync::Arc;

    fn mk_task(id: &str, due: Option<&str>) -> CalendarTask {
        let mut t = CalendarTask::new(id, "A-08 测试任务");
        t.due = due.map(|d| d.into());
        t.created_at = 1_736_100_000_000;
        t
    }

    #[tokio::test]
    async fn sync_upsert_delete_diff() {
        let kv = Arc::new(MemoryKVStore::default());
        let (w, _) = sync_tasks(
            kv.as_ref(),
            &[mk_task("t1", Some("2026-11-10")), mk_task("t2", None)],
        )
        .await
        .unwrap();
        assert_eq!(w, 2);
        // 全量再同步: t1 改期,t2 删除,t3 新增
        let (w, d) = sync_tasks(
            kv.as_ref(),
            &[mk_task("t1", Some("2026-11-12")), mk_task("t3", None)],
        )
        .await
        .unwrap();
        assert_eq!((w, d), (2, 1));
        let all = list_tasks(kv.as_ref()).await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|t| t.id == "t3" && t.due.is_none()));
    }

    #[tokio::test]
    async fn reschedule_publishes_event() {
        let kv = Arc::new(MemoryKVStore::default());
        sync_tasks(kv.as_ref(), &[mk_task("t1", Some("2026-11-10"))])
            .await
            .unwrap();
        let store = Arc::new(InMemoryEventQueue::new());
        let bus = LayeredEventBus::new(Some(store.clone()));
        reschedule_task(kv.as_ref(), &bus, "t1", Some("2026-11-13".into()))
            .await
            .unwrap();
        let all = list_tasks(kv.as_ref()).await.unwrap();
        assert_eq!(all[0].due.as_deref(), Some("2026-11-13"));
        let pending = store.pending().unwrap();
        assert_eq!(pending[0].event_type, "TaskRescheduled");
    }

    #[tokio::test]
    async fn reschedule_ghost_fails_closed() {
        let kv = Arc::new(MemoryKVStore::default());
        let bus = LayeredEventBus::new(None);
        let r = reschedule_task(kv.as_ref(), &bus, "ghost", Some("2026-11-13".into())).await;
        assert!(r.is_err(), "幽灵改期必须 fail-closed,禁止静默成功");
    }

    #[tokio::test]
    async fn empty_text_rejected() {
        let kv = Arc::new(MemoryKVStore::default());
        let mut t = mk_task("t1", None);
        t.text = "  ".into();
        assert!(sync_tasks(kv.as_ref(), &[t]).await.is_err());
    }
}
