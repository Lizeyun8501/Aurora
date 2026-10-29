//! DK-CALENDAR-02: 日程事件存储（core 面 S1）。
//!
//! Event 为笔记生态一等公民（用户裁决 2026-10-29）——KV 直存 JSON
//! （`event:{id}` 键;scan_prefix 列全+内存过滤（量级:个人日程 O(10^3)。
//! 与 Note 的 Loro 双写不同——Event 无富文本/协同面,KV 权威即终态
//! （与 trash 标记同模式:list_trashed 惯例——serde 解析失败 warn 跳过
//! 不阻塞批次）。
//!
//! 签名走 `&dyn KVStore` 直注（非 WriteContext——Event 无 seal/cipher/
//! attachments 依赖;移动端 fallback/桌面 vault 同走 core.kv_store）。
//!
//! 03 卡扩展位: repeat 字段已序列化占位（引擎未消费）;query_by_range
//! 展开重复实例的入口预留（03 落）。

use aurora_entity::Event;

use crate::error_codes::ErrorCode;
use crate::traits::kv_store::KVStore;
use crate::Error;

const EVENT_PREFIX: &str = "event:";

fn event_key(id: &str) -> String {
    format!("{EVENT_PREFIX}{id}")
}

fn domain_err(code: ErrorCode) -> Error {
    Error::Domain {
        code,
        message: code.user_message(),
    }
}

/// 新建事件——返回 event_id（UUID v4）。
/// 校验 fail-closed: 空标题拒（A04 惯例）;end < start 拒;瞬时（end=start）合法。
/// all_day=true 时调用方传当日本地 00:00 折算后的 UTC（core 不强校验——
/// 跨时区语义 03 卡重复引擎统一）。
pub async fn create_event(kv: &dyn KVStore, event: &Event) -> Result<String, Error> {
    if event.title.trim().is_empty() {
        return Err(domain_err(ErrorCode::A04));
    }
    if let Some(end) = event.end {
        if end < event.start {
            return Err(domain_err(ErrorCode::A04));
        }
    }
    let id = if event.id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        event.id.clone()
    };
    let mut ev = event.clone();
    ev.id = id.clone();
    let now = chrono::Utc::now();
    ev.created_at = now;
    ev.updated_at = now;
    let bytes = serde_json::to_vec(&ev)
        .map_err(|e| Error::Internal(format!("event serialize failed: {e}")))?;
    kv.set(&event_key(&id), &bytes).await?;
    Ok(id)
}

/// 全量列表（start 升序;解析失败 warn 跳过——批次不阻塞惯例）。
pub async fn list_events(kv: &dyn KVStore) -> Result<Vec<Event>, Error> {
    let mut out = Vec::new();
    for (key, v) in kv.scan_prefix(EVENT_PREFIX).await? {
        match serde_json::from_slice::<Event>(&v) {
            Ok(ev) => out.push(ev),
            Err(e) => tracing::warn!(key = %key, error = %e, "event parse failed; skipped"),
        }
    }
    out.sort_by(|a, b| a.start.cmp(&b.start));
    Ok(out)
}

/// 全量更新（id 必须已存在——幽灵更新 NoteNotFound fail-closed）。
/// 校验同 create（空标题/end<start 拒）;updated_at 刷新（created_at 保留原值）。
pub async fn update_event(kv: &dyn KVStore, event: &Event) -> Result<(), Error> {
    if event.title.trim().is_empty() {
        return Err(domain_err(ErrorCode::A04));
    }
    if let Some(end) = event.end {
        if end < event.start {
            return Err(domain_err(ErrorCode::A04));
        }
    }
    let key = event_key(&event.id);
    let Some(old) = kv.get(&key).await? else {
        return Err(Error::NoteNotFound { id: event.id.clone() });
    };
    let old: Event = serde_json::from_slice(&old)
        .map_err(|e| Error::Internal(format!("event parse failed: {e}")))?;
    let mut ev = event.clone();
    ev.created_at = old.created_at; // 不可变字段保留
    ev.updated_at = chrono::Utc::now();
    let bytes = serde_json::to_vec(&ev)
        .map_err(|e| Error::Internal(format!("event serialize failed: {e}")))?;
    kv.set(&key, &bytes).await?;
    Ok(())
}

/// 删除（幂等语义同 trash 系——不存在也 Ok,清场操作）。
pub async fn delete_event(kv: &dyn KVStore, event_id: &str) -> Result<(), Error> {
    kv.delete(&event_key(event_id)).await?;
    Ok(())
}

/// 单查（不存在返 None——FFI upsert 的 created_at 保留前置读）。
pub async fn get_event(kv: &dyn KVStore, event_id: &str) -> Result<Option<Event>, Error> {
    match kv.get(&event_key(event_id)).await? {
        None => Ok(None),
        Some(bytes) => Ok(Some(
            serde_json::from_slice(&bytes)
                .map_err(|e| Error::Internal(format!("event parse failed: {e}")))?,
        )),
    }
}

/// 创建/更新二合一（FFI 便捷入口）——id 空则建新（返新 id）;
/// id 非空且存在→更新;id 非空且不存在→NoteNotFound fail-closed
/// （防幽灵覆盖——同步冲突场景由上层显式 create）。
/// created_at 保留语义内聚（更新分支读旧值）。
pub async fn upsert_event(kv: &dyn KVStore, event: &Event) -> Result<String, Error> {
    if event.id.is_empty() {
        create_event(kv, event).await
    } else if get_event(kv, &event.id).await?.is_some() {
        update_event(kv, event).await.map(|_| event.id.clone())
    } else {
        Err(Error::NoteNotFound { id: event.id.clone() })
    }
}

/// 按范围查询 [from, to)——含跨月边界: 事件与区间相交即命中
/// （start < to && end >= from;end=None=瞬时用 start 折算——即
/// from <= start < to）。start 升序。
/// 03 卡: repeat 展开实例的入口在此挂（expand=true 时由重复引擎
/// 生成虚拟实例,本版恒不展开——诚实边界）。
pub async fn query_events_by_range(
    kv: &dyn KVStore,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<Event>, Error> {
    let mut out = Vec::new();
    for ev in list_events(kv).await? {
        let ev_end = ev.end.unwrap_or(ev.start); // 瞬时=起点
        let hit = ev.start < to && ev_end >= from;
        if hit {
            out.push(ev);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod event_tests {
    use super::*;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use aurora_entity::RepeatFreq;
    use aurora_entity::RepeatRule;
    use chrono::TimeZone;

    fn kv() -> std::sync::Arc<MemoryKVStore> {
        std::sync::Arc::new(MemoryKVStore::default())
    }

    fn ev(id: &str, title: &str, y: i32, mo: u32, d: u32, h: u32) -> Event {
        Event::new(
            id,
            title,
            chrono::Utc.with_ymd_and_hms(y, mo, d, h, 0, 0).unwrap(),
        )
    }

    #[tokio::test]
    async fn crud_roundtrip() {
        let kv = kv();
        // create（空 id→UUID 生成
        let e = ev("", "晨会", 2026, 10, 30, 9);
        let id = create_event(kv.as_ref(), &e).await.unwrap();
        assert!(!id.is_empty());

        // list
        let all = list_events(kv.as_ref()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "晨会");
        assert_eq!(all[0].id, id);
        assert!(all[0].note_id.is_none() && all[0].color == 0);

        // update（标题+色+关联笔记
        let mut e2 = all[0].clone();
        e2.title = "晨会（改期".to_string();
        e2.color = 3;
        e2.note_id = Some("note-1".into());
        let old_created = e2.created_at;
        update_event(kv.as_ref(), &e2).await.unwrap();
        let all2 = list_events(kv.as_ref()).await.unwrap();
        assert_eq!(all2[0].title, "晨会（改期");
        assert_eq!(all2[0].color, 3);
        assert_eq!(all2[0].note_id.as_deref(), Some("note-1"));
        assert_eq!(all2[0].created_at, old_created, "created_at 不可变");

        // delete（幂等: 二删不报错
        delete_event(kv.as_ref(), &id).await.unwrap();
        assert!(list_events(kv.as_ref()).await.unwrap().is_empty());
        delete_event(kv.as_ref(), &id).await.unwrap();
    }

    #[tokio::test]
    async fn range_cross_month() {
        let kv = kv();
        // 三个事件: 9 月末 / 10 月中 / 10-31→11-01 跨月
        create_event(kv.as_ref(), &ev("", "九月旧事", 2026, 9, 30, 23)).await.unwrap();
        create_event(kv.as_ref(), &ev("", "十月例会", 2026, 10, 15, 10)).await.unwrap();
        let mut night = ev("", "跨月通宵", 2026, 10, 31, 22);
        night.end = Some(chrono::Utc.with_ymd_and_hms(2026, 11, 1, 6, 0, 0).unwrap());
        create_event(kv.as_ref(), &night).await.unwrap();

        // 查 10 月（含 10-31 尾格→11-01 头格边界
        let from = chrono::Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let to = chrono::Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let oct = query_events_by_range(kv.as_ref(), from, to).await.unwrap();
        assert_eq!(oct.len(), 2, "十月=例会+跨月通宵（9 月旧事出局");
        assert!(oct.iter().any(|e| e.title == "跨月通宵"));

        // 查 11 月（跨月通宵仍在（end>=11-01 from
        let nov_from = to;
        let nov_to = chrono::Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap();
        let nov = query_events_by_range(kv.as_ref(), nov_from, nov_to).await.unwrap();
        assert_eq!(nov.len(), 1, "跨月事件命中 11 月视图（start<to 且 end>=from");
        assert_eq!(nov[0].title, "跨月通宵");

        // 9 月事件不在 10 月
        assert!(oct.iter().all(|e| e.title != "九月旧事"));
    }

    #[tokio::test]
    async fn validation_fail_closed() {
        let kv = kv();
        // 空标题拒绝
        let bad = ev("", "  ", 2026, 10, 30, 9);
        assert!(create_event(kv.as_ref(), &bad).await.is_err());
        // end < start 拒绝
        let mut bad2 = ev("", "倒流", 2026, 10, 30, 9);
        bad2.end = Some(chrono::Utc.with_ymd_and_hms(2026, 10, 30, 8, 0, 0).unwrap());
        assert!(create_event(kv.as_ref(), &bad2).await.is_err());
        // 更新不存在的事件报 NoteNotFound
        let ghost = ev("ghost-404", "幽灵", 2026, 10, 30, 9);
        assert!(update_event(kv.as_ref(), &ghost).await.is_err());
        // 瞬时事件合法（start=end 同刻
        let mut inst = ev("", "瞬时", 2026, 10, 30, 12);
        inst.end = Some(inst.start);
        assert!(create_event(kv.as_ref(), &inst).await.is_ok());
    }

    #[tokio::test]
    async fn repeat_field_roundtrip() {
        // 03 卡占位: RepeatRule 序列化面（core 不消费,只保证存储往返无损
        let kv = kv();
        let mut e = ev("", "周例会", 2026, 10, 30, 9);
        e.repeat = Some(RepeatRule {
            freq: RepeatFreq::Weekly,
            interval: 1,
            until: None,
        });
        let id = create_event(kv.as_ref(), &e).await.unwrap();
        let all = list_events(kv.as_ref()).await.unwrap();
        assert_eq!(all[0].repeat, e.repeat, "repeat 占位字段往返无损");
        assert_eq!(all[0].id, id);
    }
}
