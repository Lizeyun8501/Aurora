//! A-08: CalendarVm 组装——双端日历统一数据源（Rust 输出,前端只渲染）。
//!
//! VM 面 = events（event_store 全量,重复展开由消费端 shared repeatUtil
//! 单份实现）+ tasks（task_store 全量,带 due 的日历消费面）。
//!
//! **差异披露（按真实通路裁剪卡面字段）**: 卡面 CalendarVm 含
//! selectedDate/viewMode——实况为 UI 本地态（跨端各异,无持久价值）,
//! 落 Rust 会制造状态回环;VM 保持纯数据面,选择/视图态由各端 VM hook
//! 自持。数据同源 ✓（双端读同一 build_calendar_vm 输出）。

use aurora_entity::Event;

use crate::task_store::CalendarTask;
use crate::traits::kv_store::KVStore;
use crate::Error;

/// 双端日历统一 VM（Rust 输出;序列化 snake_case——TS 镜像
/// `shared/types/src/calendar.ts` CalendarVm 同构）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CalendarVm {
    /// 日程事件全量（start 升序;repeat 原始规则——展开在消费端）。
    pub events: Vec<Event>,
    /// 任务全量（带 due 的日历消费面;created_at 降序）。
    pub tasks: Vec<CalendarTask>,
}

/// 组装 CalendarVm（events + tasks 全量读取——个人量级 O(10^3) 直读;
/// 子读取错误原样上抛,上层 fail-closed——无自产 domain 错误路径）。
pub async fn build_calendar_vm(kv: &dyn KVStore) -> Result<CalendarVm, Error> {
    let events = crate::event_store::list_events(kv).await?;
    let tasks = crate::task_store::list_tasks(kv).await?;
    Ok(CalendarVm { events, tasks })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use crate::task_store::sync_tasks;
    use chrono::TimeZone;
    use std::sync::Arc;

    #[tokio::test]
    async fn vm_aggregates_events_and_tasks() {
        let kv = Arc::new(MemoryKVStore::default());
        let mut ev = aurora_entity::Event::new(
            "e1",
            "A-08 事件",
            chrono::Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap(),
        );
        ev.repeat = None;
        crate::event_store::create_event(kv.as_ref(), &ev)
            .await
            .unwrap();
        let mut t = crate::task_store::CalendarTask::new("t1", "A-08 任务");
        t.due = Some("2026-11-10".into());
        crate::task_store::sync_tasks(kv.as_ref(), &[t])
            .await
            .unwrap();

        let vm = build_calendar_vm(kv.as_ref()).await.unwrap();
        assert_eq!(vm.events.len(), 1);
        assert_eq!(vm.tasks.len(), 1);
        assert_eq!(vm.tasks[0].due.as_deref(), Some("2026-11-10"));
        // JSON 往返 snake_case（TS 镜像契约）
        let j = serde_json::to_value(&vm).unwrap();
        assert!(j["tasks"][0]["created_at"].is_number());
        assert!(j["events"][0]["note_id"].is_null() || j["events"][0]["note_id"].is_string());
    }

    #[tokio::test]
    async fn vm_empty_is_legitimate_not_placeholder() {
        // 空库=真空集（个人量级语义）,非静默占位: 区分错误与真空靠 Result,
        // Err 时上层必须抛错（铁律 2）——这里只验证 Ok(空) 形态。
        let kv = Arc::new(MemoryKVStore::default());
        let vm = build_calendar_vm(kv.as_ref()).await.unwrap();
        assert!(vm.events.is_empty() && vm.tasks.is_empty());
    }
}
