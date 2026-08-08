//! 日历同步 (Calendar Sync, CalDAV RFC 4791)
//!
//! 实现 CalDAV 日历与 Aurora GTD 任务的双向同步：
//! - GTD 任务 (`due_date`, `title`) ↔ 日历 VEVENT (`DTSTART`, `SUMMARY`)。
//! - 增量同步基于 [`Ctag`] (calendar-wide) + [`Etag`] (per-event)，
//!   CTag 未变则跳过整轮；CTag 变化后逐条比对 ETag 定位变更事件。
//! - 双向冲突：同一 UID 本地与远端均发生修改时，由 [`CalendarSync`] 检测，
//!   默认采用 last-write-wins (LWW) 策略解决。
//!
//! # 实现说明
//! [`CalDavConnector`] 内部以 `HashMap` 模拟 CalDAV 服务端日历存储，
//! 真实实现替换 `events` / `ctag` 操作为 HTTPS + iCal 解析即可，公开 API 不变。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::{ConnectorState, SyncConnector, SyncSession};

/// CalDAV 日历配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalDavConfig {
    /// CalDAV 服务端 base URL (如 `https://caldav.example/`).
    pub url: String,
    /// 日历集合路径 (如 `calendars/user/aurora/`).
    pub calendar_path: String,
    /// 用户名。
    pub username: String,
    /// 密码 / 应用专用密码。
    pub password: String,
    /// 轮询间隔 (秒)。
    pub poll_interval_secs: u64,
}

impl Default for CalDavConfig {
    fn default() -> Self {
        Self {
            url: "https://caldav.aurora.example/".to_string(),
            calendar_path: "calendars/aurora/default/".to_string(),
            username: "aurora".to_string(),
            password: String::new(),
            poll_interval_secs: 300,
        }
    }
}

/// Calendar-wide CTag (RFC 4791 `sync-token` 语义等价物)。
///
/// CTag 在日历任意事件变化时递增/变更，客户端缓存上次同步的 CTag，
/// 若服务端 CTag 未变则跳过本轮同步。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ctag(pub String);

impl Ctag {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 单个 VEVENT 的 ETag (版本指纹)。
///
/// ETag 在事件内容变化时变更，用于增量比对定位修改过的事件。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Etag(pub String);

impl Etag {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 日历事件 (VEVENT 投影)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// VEVENT `UID`。
    pub uid: String,
    /// `SUMMARY` (标题)。
    pub summary: String,
    /// `DESCRIPTION` (描述)。
    pub description: Option<String>,
    /// `DTSTART` (开始时间)。
    pub dtstart: chrono::DateTime<chrono::Utc>,
    /// `DTEND` (结束时间)。
    pub dtend: chrono::DateTime<chrono::Utc>,
    /// `LOCATION` (地点)。
    pub location: Option<String>,
    /// `LAST-MODIFIED`。
    pub last_modified: chrono::DateTime<chrono::Utc>,
    /// 服务端返回的 ETag (本地新建事件为 None)。
    pub etag: Option<Etag>,
}

impl CalendarEvent {
    /// 创建新事件 (默认 1 小时时长)。
    pub fn new(
        uid: impl Into<String>,
        summary: impl Into<String>,
        dtstart: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let dtend = dtstart + chrono::Duration::hours(1);
        let now = chrono::Utc::now();
        Self {
            uid: uid.into(),
            summary: summary.into(),
            description: None,
            dtstart,
            dtend,
            location: None,
            last_modified: now,
            etag: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_location(mut self, loc: impl Into<String>) -> Self {
        self.location = Some(loc.into());
        self
    }

    pub fn with_etag(mut self, etag: Etag) -> Self {
        self.etag = Some(etag);
        self
    }

    /// 转换为 GTD 任务。
    ///
    /// 映射规则：
    /// - `uid` → task id (加 `caldav:` 前缀，避免与本地任务 ID 碰撞)
    /// - `summary` → `title`
    /// - `description` → `description`
    /// - `dtstart` → `scheduled_date`
    /// - `dtend` → `due_date` (事件结束视为任务截止)
    /// - `last_modified` → `updated_at`
    pub fn to_gtd_task(&self) -> aurora_core::l3_domain::gtd_system::Task {
        use aurora_core::l3_domain::gtd_system::{Task, TaskStatus};
        let mut task = Task::new(self.summary.clone());
        task.id = format!("caldav:{}", self.uid);
        task.description = self.description.clone();
        task.status = TaskStatus::Scheduled;
        task.scheduled_date = Some(self.dtstart);
        task.due_date = Some(self.dtend);
        task.updated_at = self.last_modified;
        task.created_at = self.last_modified;
        task
    }
}

/// 将 GTD 任务转换为日历事件。
///
/// 映射规则：
/// - `title` → `summary`
/// - `description` → `description`
/// - `due_date` → `dtstart` (无 due_date 时回退到 `scheduled_date`)
/// - `dtend` = `dtstart` + 1 小时
/// - `id` 去除 `caldav:` 前缀后作为 `uid`
pub fn task_to_event(task: &aurora_core::l3_domain::gtd_system::Task) -> CalendarEvent {
    let dtstart = task
        .due_date
        .or(task.scheduled_date)
        .unwrap_or_else(chrono::Utc::now);
    let uid = task
        .id
        .strip_prefix("caldav:")
        .map(|s| s.to_string())
        .unwrap_or_else(|| task.id.clone());
    let mut event = CalendarEvent::new(uid, task.title.clone(), dtstart);
    if let Some(desc) = &task.description {
        event = event.with_description(desc.clone());
    }
    event.last_modified = task.updated_at;
    event
}

/// 远端变更集合 (增量检测结果)。
#[derive(Debug, Clone, Default)]
pub struct ChangeSet {
    /// 新增或更新的事件。
    pub updated: Vec<CalendarEvent>,
    /// 已删除的 UID。
    pub deleted: Vec<String>,
}

impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.updated.is_empty() && self.deleted.is_empty()
    }

    pub fn total(&self) -> usize {
        self.updated.len() + self.deleted.len()
    }
}

/// 双向同步冲突描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarConflict {
    pub uid: String,
    pub local_etag: Option<Etag>,
    pub remote_etag: Option<Etag>,
}

/// CalDAV 连接器 (mock CalDAV 服务端)。
///
/// 内存模拟一个日历集合：`UID -> (Event, ETag)`，并维护 calendar-wide CTag。
/// 任何 `put_event` / `remove_event` 都会刷新对应 ETag 与 CTag。
pub struct CalDavConnector {
    name: String,
    config: CalDavConfig,
    state: Arc<RwLock<ConnectorState>>,
    /// uid -> (event, etag)
    events: Arc<RwLock<HashMap<String, (CalendarEvent, Etag)>>>,
    ctag: Arc<RwLock<Ctag>>,
}

impl CalDavConnector {
    pub fn new(name: impl Into<String>, config: CalDavConfig) -> Self {
        Self {
            name: name.into(),
            config,
            state: Arc::new(RwLock::new(ConnectorState::Disconnected)),
            events: Arc::new(RwLock::new(HashMap::new())),
            ctag: Arc::new(RwLock::new(Ctag(uuid::Uuid::new_v4().to_string()))),
        }
    }

    pub fn config(&self) -> &CalDavConfig {
        &self.config
    }

    /// 当前日历 CTag。
    pub fn current_ctag(&self) -> Ctag {
        self.ctag.read().clone()
    }

    /// 服务端写入 / 更新事件，刷新 ETag 与 CTag。
    pub fn put_event(&self, event: CalendarEvent) -> Etag {
        let etag = Etag(uuid::Uuid::new_v4().to_string());
        let mut ev = event;
        ev.last_modified = chrono::Utc::now();
        let uid = ev.uid.clone();
        let etag_clone = etag.clone();
        ev.etag = Some(etag_clone);
        self.events.write().insert(uid.clone(), (ev, etag.clone()));
        // 任何写入都刷新 CTag
        *self.ctag.write() = Ctag(uuid::Uuid::new_v4().to_string());
        debug!("caldav put_event: uid={} new_ctag", uid);
        etag
    }

    /// 服务端删除事件，刷新 CTag。
    pub fn remove_event(&self, uid: &str) -> bool {
        let removed = self.events.write().remove(uid).is_some();
        if removed {
            *self.ctag.write() = Ctag(uuid::Uuid::new_v4().to_string());
            debug!("caldav remove_event: uid={}", uid);
        }
        removed
    }

    /// 获取单个事件 (附带最新 ETag)。
    pub fn get_event(&self, uid: &str) -> Option<CalendarEvent> {
        self.events.read().get(uid).map(|(e, _)| e.clone())
    }

    /// 获取事件对应的 ETag。
    pub fn get_etag(&self, uid: &str) -> Option<Etag> {
        self.events.read().get(uid).map(|(_, t)| t.clone())
    }

    /// 列出全部事件。
    pub fn list_events(&self) -> Vec<CalendarEvent> {
        self.events
            .read()
            .values()
            .map(|(e, _)| e.clone())
            .collect()
    }

    /// 服务端事件总数。
    pub fn event_count(&self) -> usize {
        self.events.read().len()
    }
}

impl SyncConnector for CalDavConnector {
    fn name(&self) -> &str {
        &self.name
    }
    fn provider(&self) -> &str {
        "caldav"
    }
    fn connect(&self) -> crate::Result<()> {
        *self.state.write() = ConnectorState::Connecting;
        // mock: 模拟 BASIC auth 握手成功
        if self.config.url.is_empty() {
            *self.state.write() = ConnectorState::Error("empty url".into());
            return Err(crate::Error::ExternalSync("empty caldav url".into()));
        }
        *self.state.write() = ConnectorState::Connected;
        info!("caldav connected: {}", self.name);
        Ok(())
    }
    fn disconnect(&self) -> crate::Result<()> {
        *self.state.write() = ConnectorState::Disconnected;
        info!("caldav disconnected: {}", self.name);
        Ok(())
    }
    fn sync(&self) -> crate::Result<SyncSession> {
        if !self.state.read().is_connected() {
            return Err(crate::Error::ExternalSync(format!(
                "connector not connected: {}",
                self.name
            )));
        }
        let count = self.event_count();
        let mut session = SyncSession::new(self.name.clone(), "caldav");
        session.finish(count, 0);
        Ok(session)
    }
    fn state(&self) -> ConnectorState {
        self.state.read().clone()
    }
}

/// 日历同步引擎。
///
/// 维护本地事件副本、上次同步 CTag 与 per-UID ETag，并提供：
/// - [`detect_remote_changes`](Self::detect_remote_changes)：增量检测远端变更。
/// - [`pull_remote`](Self::pull_remote)：拉取远端变更到本地。
/// - [`push_local`](Self::push_local)：推送本地修改到远端。
/// - [`detect_conflicts`](Self::detect_conflicts)：检测双向冲突。
/// - [`full_sync`](Self::full_sync)：完整双向同步 (LWW 解决冲突)。
pub struct CalendarSync {
    connector: Arc<CalDavConnector>,
    local_events: Arc<RwLock<HashMap<String, CalendarEvent>>>,
    local_ctag: Arc<RwLock<Option<Ctag>>>,
    local_etags: Arc<RwLock<HashMap<String, Etag>>>,
    /// 自上次同步以来本地修改过的 UID。
    locally_modified: Arc<RwLock<HashSet<String>>>,
}

impl CalendarSync {
    pub fn new(connector: Arc<CalDavConnector>) -> Self {
        Self {
            connector,
            local_events: Arc::new(RwLock::new(HashMap::new())),
            local_ctag: Arc::new(RwLock::new(None)),
            local_etags: Arc::new(RwLock::new(HashMap::new())),
            locally_modified: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// 本地写入 / 更新事件 (标记为本地修改)。
    pub fn local_put(&self, event: CalendarEvent) {
        let uid = event.uid.clone();
        self.local_events.write().insert(uid.clone(), event);
        self.locally_modified.write().insert(uid);
    }

    /// 本地删除事件 (标记为本地修改)。
    pub fn local_remove(&self, uid: &str) -> bool {
        let removed = self.local_events.write().remove(uid).is_some();
        if removed {
            self.locally_modified.write().insert(uid.to_string());
        }
        removed
    }

    /// 本地事件数量。
    pub fn local_count(&self) -> usize {
        self.local_events.read().len()
    }

    /// 获取本地事件。
    pub fn local_get(&self, uid: &str) -> Option<CalendarEvent> {
        self.local_events.read().get(uid).cloned()
    }

    /// 上次同步记录的 CTag。
    pub fn last_ctag(&self) -> Option<Ctag> {
        self.local_ctag.read().clone()
    }

    /// 增量检测远端变更。
    ///
    /// 若远端 CTag 与本地记录一致则返回空集；否则逐条比对 ETag。
    pub fn detect_remote_changes(&self) -> ChangeSet {
        let remote_ctag = self.connector.current_ctag();
        let known_ctag = self.local_ctag.read().clone();
        if known_ctag.as_ref() == Some(&remote_ctag) {
            return ChangeSet::default();
        }
        let remote_events = self.connector.list_events();
        let known_etags = self.local_etags.read();
        let mut updated = Vec::new();
        for ev in &remote_events {
            let known = known_etags.get(&ev.uid);
            match known {
                Some(t) if Some(t) == ev.etag.as_ref() => {} // 未变
                _ => updated.push(ev.clone()),
            }
        }
        // 远端已删除：本地有记录但远端不再存在
        let remote_uids: HashSet<String> = remote_events.iter().map(|e| e.uid.clone()).collect();
        let mut deleted = Vec::new();
        for uid in known_etags.keys() {
            if !remote_uids.contains(uid) {
                deleted.push(uid.clone());
            }
        }
        ChangeSet { updated, deleted }
    }

    /// 检测双向冲突：同时被本地与远端修改的 UID。
    pub fn detect_conflicts(&self) -> Vec<CalendarConflict> {
        let changes = self.detect_remote_changes();
        let modified = self.locally_modified.read();
        let mut conflicts = Vec::new();
        for ev in &changes.updated {
            if modified.contains(&ev.uid) {
                let local_etag = self
                    .local_events
                    .read()
                    .get(&ev.uid)
                    .and_then(|e| e.etag.clone());
                conflicts.push(CalendarConflict {
                    uid: ev.uid.clone(),
                    local_etag,
                    remote_etag: ev.etag.clone(),
                });
            }
        }
        conflicts
    }

    /// 拉取远端变更到本地 (应用 ChangeSet)。
    ///
    /// 返回应用的事件数 (新增 + 更新 + 删除)。
    pub fn pull_remote(&self) -> crate::Result<usize> {
        let changes = self.detect_remote_changes();
        let mut applied = 0;
        {
            let mut events = self.local_events.write();
            let mut etags = self.local_etags.write();
            for ev in &changes.updated {
                let uid = ev.uid.clone();
                let etag = ev.etag.clone().unwrap_or_else(|| Etag("unknown".into()));
                events.insert(uid.clone(), ev.clone());
                etags.insert(uid, etag);
                applied += 1;
            }
            for uid in &changes.deleted {
                events.remove(uid);
                etags.remove(uid);
                applied += 1;
            }
        }
        // 拉取后同步 CTag
        *self.local_ctag.write() = Some(self.connector.current_ctag());
        // 远端拉取覆盖后，本地修改标记对未冲突项清除
        {
            let mut modified = self.locally_modified.write();
            for ev in &changes.updated {
                // 仅清除非冲突项 (冲突由 full_sync 单独处理)
                modified.remove(&ev.uid);
            }
        }
        if applied > 0 {
            info!("caldav pull_remote: applied={} changes", applied);
        }
        Ok(applied)
    }

    /// 推送本地修改到远端。
    pub fn push_local(&self) -> crate::Result<usize> {
        let to_push: Vec<CalendarEvent> = {
            let modified = self.locally_modified.read();
            let events = self.local_events.read();
            modified
                .iter()
                .filter_map(|uid| events.get(uid).cloned())
                .collect()
        };
        // 区分删除：locally_modified 中不在 local_events 视为本地删除
        let to_delete: Vec<String> = {
            let modified = self.locally_modified.read();
            let events = self.local_events.read();
            modified
                .iter()
                .filter(|uid| !events.contains_key(*uid))
                .cloned()
                .collect()
        };
        let mut pushed = 0;
        for ev in &to_push {
            let etag = self.connector.put_event(ev.clone());
            // 回写本地 etag
            let uid = ev.uid.clone();
            if let Some(e) = self.local_events.write().get_mut(&uid) {
                e.etag = Some(etag.clone());
            }
            self.local_etags.write().insert(uid.clone(), etag);
            pushed += 1;
        }
        for uid in &to_delete {
            self.connector.remove_event(uid);
            self.local_etags.write().remove(uid);
            pushed += 1;
        }
        // 仅清除本地修改标记；local_ctag 由 pull_remote 在拉取远端变更后推进，
        // 此处不更新以避免跳过尚未拉取的远端变更。
        self.locally_modified.write().clear();
        if pushed > 0 {
            info!("caldav push_local: pushed={} items", pushed);
        }
        Ok(pushed)
    }

    /// 完整双向同步：先 push，再 pull，冲突采用 LWW (last-write-wins)。
    ///
    /// 返回 (同步条目数, 冲突数)。
    pub fn full_sync(&self) -> crate::Result<(usize, usize)> {
        // 先检测冲突并解决 (LWW)
        let conflicts = self.detect_conflicts();
        let conflict_count = conflicts.len();
        for c in &conflicts {
            let local = self.local_events.read().get(&c.uid).cloned();
            let remote = self.connector.get_event(&c.uid);
            match (local, remote) {
                (Some(l), Some(r)) => {
                    // LWW: 比较最后修改时间
                    if l.last_modified >= r.last_modified {
                        // 本地较新，推送本地覆盖远端
                        self.connector.put_event(l);
                    } else {
                        // 远端较新，拉取远端覆盖本地
                        self.local_events.write().insert(c.uid.clone(), r);
                    }
                }
                (Some(l), None) => {
                    // 远端已删除但本地仍修改 → 推送本地 (恢复)
                    self.connector.put_event(l);
                }
                _ => {}
            }
            // 清除该 UID 的本地修改标记
            self.locally_modified.write().remove(&c.uid);
        }
        if conflict_count > 0 {
            warn!(
                "caldav full_sync: resolved {} conflicts (LWW)",
                conflict_count
            );
        }
        let pushed = self.push_local()?;
        let pulled = self.pull_remote()?;
        Ok((pushed + pulled, conflict_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(year: i32, month: u32, day: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, year, month, day, 9, 0, 0).unwrap()
    }

    fn make_connector() -> CalDavConnector {
        CalDavConnector::new("cal", CalDavConfig::default())
    }

    #[test]
    fn test_caldav_config_default() {
        let cfg = CalDavConfig::default();
        assert!(cfg.url.starts_with("https://"));
        assert!(!cfg.calendar_path.is_empty());
        assert!(cfg.poll_interval_secs > 0);
    }

    #[test]
    fn test_ctag_etag_newtypes() {
        let c = Ctag::new("v1");
        assert_eq!(c.as_str(), "v1");
        let e = Etag::new("r1");
        assert_eq!(e.as_str(), "r1");
        assert_eq!(Ctag::new("v1"), Ctag::new("v1"));
    }

    #[test]
    fn test_calendar_event_builders() {
        let ev = CalendarEvent::new("u1", "Meeting", dt(2026, 1, 1))
            .with_description("sync")
            .with_location("Room A");
        assert_eq!(ev.uid, "u1");
        assert_eq!(ev.summary, "Meeting");
        assert_eq!(ev.description.as_deref(), Some("sync"));
        assert_eq!(ev.location.as_deref(), Some("Room A"));
        // 默认时长 1 小时
        assert_eq!(ev.dtend - ev.dtstart, chrono::Duration::hours(1));
        assert!(ev.etag.is_none());
    }

    #[test]
    fn test_event_to_gtd_task_mapping() {
        let ev = CalendarEvent::new("evt-1", "Standup", dt(2026, 3, 1)).with_description("daily");
        let task = ev.to_gtd_task();
        assert_eq!(task.id, "caldav:evt-1");
        assert_eq!(task.title, "Standup");
        assert_eq!(task.description.as_deref(), Some("daily"));
        assert_eq!(task.scheduled_date, Some(ev.dtstart));
        assert_eq!(task.due_date, Some(ev.dtend));
    }

    #[test]
    fn test_task_to_event_mapping_roundtrip() {
        use aurora_core::l3_domain::gtd_system::Task;
        let mut task = Task::new("Write report");
        task.id = "caldav:evt-9".to_string();
        task = task.with_due_date(dt(2026, 5, 10));
        task.description = Some("desc".into());

        let event = task_to_event(&task);
        assert_eq!(event.uid, "evt-9"); // 去除前缀
        assert_eq!(event.summary, "Write report");
        assert_eq!(event.description.as_deref(), Some("desc"));
        assert_eq!(event.dtstart, dt(2026, 5, 10));
        // roundtrip 回 task 时 id 重新加上前缀
        let back = event.to_gtd_task();
        assert_eq!(back.id, "caldav:evt-9");
        assert_eq!(back.title, "Write report");
    }

    #[test]
    fn test_task_to_event_falls_back_to_scheduled_date() {
        use aurora_core::l3_domain::gtd_system::Task;
        let mut task = Task::new("No due");
        // 仅设置 scheduled_date，未设置 due_date
        task.scheduled_date = Some(dt(2026, 6, 1));
        let event = task_to_event(&task);
        assert_eq!(event.dtstart, dt(2026, 6, 1));
    }

    #[test]
    fn test_connector_put_event_bumps_etag_and_ctag() {
        let conn = make_connector();
        let ctag0 = conn.current_ctag();
        let etag1 = conn.put_event(CalendarEvent::new("u1", "A", dt(2026, 1, 1)));
        let ctag1 = conn.current_ctag();
        assert_ne!(ctag0, ctag1);
        let etag2 = conn.put_event(CalendarEvent::new("u1", "A-updated", dt(2026, 1, 2)));
        assert_ne!(etag1, etag2); // 更新后 ETag 变化
        assert_eq!(conn.event_count(), 1);
        // get_event 携带最新 etag
        let fetched = conn.get_event("u1").unwrap();
        assert_eq!(fetched.summary, "A-updated");
        assert_eq!(fetched.etag.as_ref(), Some(&etag2));
    }

    #[test]
    fn test_connector_remove_event_bumps_ctag() {
        let conn = make_connector();
        conn.put_event(CalendarEvent::new("u1", "A", dt(2026, 1, 1)));
        let ctag_before = conn.current_ctag();
        assert!(conn.remove_event("u1"));
        let ctag_after = conn.current_ctag();
        assert_ne!(ctag_before, ctag_after);
        assert_eq!(conn.event_count(), 0);
        assert!(!conn.remove_event("u1")); // 已不存在
    }

    #[test]
    fn test_connector_connect_disconnect_sync() {
        let conn = make_connector();
        assert_eq!(conn.state(), ConnectorState::Disconnected);
        conn.connect().unwrap();
        assert_eq!(conn.state(), ConnectorState::Connected);
        // 未连接时无法 sync：先放一个事件再 sync
        conn.put_event(CalendarEvent::new("u1", "A", dt(2026, 1, 1)));
        let session = conn.sync().unwrap();
        assert_eq!(session.items_synced, 1);
        conn.disconnect().unwrap();
        assert_eq!(conn.state(), ConnectorState::Disconnected);
        // 断开后 sync 报错
        assert!(conn.sync().is_err());
    }

    #[test]
    fn test_connector_connect_empty_url_errors() {
        let mut cfg = CalDavConfig::default();
        cfg.url = String::new();
        let conn = CalDavConnector::new("cal", cfg);
        let r = conn.connect();
        assert!(r.is_err());
        assert!(conn.state().is_error());
    }

    #[test]
    fn test_detect_changes_no_ctag_change() {
        let conn = Arc::new(make_connector());
        let sync = CalendarSync::new(conn.clone());
        conn.put_event(CalendarEvent::new("u1", "A", dt(2026, 1, 1)));
        // 首次 pull 建立 ctag baseline
        sync.pull_remote().unwrap();
        let changes = sync.detect_remote_changes();
        assert!(changes.is_empty());
    }

    #[test]
    fn test_detect_changes_etag_incremental() {
        let conn = Arc::new(make_connector());
        let sync = CalendarSync::new(conn.clone());
        conn.put_event(CalendarEvent::new("u1", "A", dt(2026, 1, 1)));
        conn.put_event(CalendarEvent::new("u2", "B", dt(2026, 1, 2)));
        sync.pull_remote().unwrap(); // baseline

        // 修改 u1，新增 u3
        conn.put_event(CalendarEvent::new("u1", "A-v2", dt(2026, 1, 3)));
        conn.put_event(CalendarEvent::new("u3", "C", dt(2026, 1, 4)));
        let changes = sync.detect_remote_changes();
        assert_eq!(changes.updated.len(), 2);
        assert!(changes.updated.iter().any(|e| e.uid == "u1"));
        assert!(changes.updated.iter().any(|e| e.uid == "u3"));
        // u2 未变，不在 updated 中
        assert!(!changes.updated.iter().any(|e| e.uid == "u2"));
    }

    #[test]
    fn test_detect_changes_deletion() {
        let conn = Arc::new(make_connector());
        let sync = CalendarSync::new(conn.clone());
        conn.put_event(CalendarEvent::new("u1", "A", dt(2026, 1, 1)));
        conn.put_event(CalendarEvent::new("u2", "B", dt(2026, 1, 2)));
        sync.pull_remote().unwrap();

        conn.remove_event("u1");
        let changes = sync.detect_remote_changes();
        assert_eq!(changes.deleted, vec!["u1".to_string()]);
        assert!(changes.updated.is_empty());
    }

    #[test]
    fn test_pull_remote_applies_changes() {
        let conn = Arc::new(make_connector());
        let sync = CalendarSync::new(conn.clone());
        conn.put_event(CalendarEvent::new("u1", "A", dt(2026, 1, 1)));
        conn.put_event(CalendarEvent::new("u2", "B", dt(2026, 1, 2)));
        let applied = sync.pull_remote().unwrap();
        assert_eq!(applied, 2);
        assert_eq!(sync.local_count(), 2);
        assert!(sync.last_ctag().is_some());
    }

    #[test]
    fn test_push_local_pushes_modified() {
        let conn = Arc::new(make_connector());
        let sync = CalendarSync::new(conn.clone());
        // 本地写入两个事件
        sync.local_put(CalendarEvent::new("u1", "A", dt(2026, 1, 1)));
        sync.local_put(CalendarEvent::new("u2", "B", dt(2026, 1, 2)));
        let pushed = sync.push_local().unwrap();
        assert_eq!(pushed, 2);
        assert_eq!(conn.event_count(), 2);
        // 推送后本地修改标记应清空
        let pushed_again = sync.push_local().unwrap();
        assert_eq!(pushed_again, 0);
    }

    #[test]
    fn test_push_local_handles_local_deletion() {
        let conn = Arc::new(make_connector());
        let sync = CalendarSync::new(conn.clone());
        sync.local_put(CalendarEvent::new("u1", "A", dt(2026, 1, 1)));
        sync.push_local().unwrap();
        assert_eq!(conn.event_count(), 1);
        // 本地删除并推送
        sync.local_remove("u1");
        let pushed = sync.push_local().unwrap();
        assert_eq!(pushed, 1);
        assert_eq!(conn.event_count(), 0); // 远端也删除
    }

    #[test]
    fn test_detect_conflicts_both_modified() {
        let conn = Arc::new(make_connector());
        let sync = CalendarSync::new(conn.clone());
        // 远端建立 baseline
        conn.put_event(CalendarEvent::new("u1", "remote-v1", dt(2026, 1, 1)));
        sync.pull_remote().unwrap();
        // 双方都修改 u1
        conn.put_event(CalendarEvent::new("u1", "remote-v2", dt(2026, 1, 2)));
        sync.local_put(CalendarEvent::new("u1", "local-v2", dt(2026, 1, 3)));
        let conflicts = sync.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].uid, "u1");
    }

    #[test]
    fn test_full_sync_resolves_conflict_lww() {
        let conn = Arc::new(make_connector());
        let sync = CalendarSync::new(conn.clone());
        // 远端 baseline
        conn.put_event(CalendarEvent::new("u1", "remote-v1", dt(2026, 1, 1)));
        sync.pull_remote().unwrap();
        // 远端修改：put_event 会把 last_modified 设为 now，因此远端时间戳为当前时间。
        let remote_ev = CalendarEvent::new("u1", "remote-v2", dt(2026, 1, 2));
        conn.put_event(remote_ev);
        // 本地修改：将 last_modified 设为未来时间，确保 LWW 选本地。
        let mut local_ev = CalendarEvent::new("u1", "local-v2", dt(2026, 1, 3));
        local_ev.last_modified = chrono::Utc::now() + chrono::Duration::days(1);
        sync.local_put(local_ev);
        let (synced, conflicts) = sync.full_sync().unwrap();
        assert_eq!(conflicts, 1);
        assert!(synced > 0);
        // 远端最终应为本地版本
        let final_ev = conn.get_event("u1").unwrap();
        assert_eq!(final_ev.summary, "local-v2");
    }

    #[test]
    fn test_changeset_helpers() {
        let cs = ChangeSet::default();
        assert!(cs.is_empty());
        assert_eq!(cs.total(), 0);
    }

    #[test]
    fn test_registry_integration_with_caldav() {
        use super::super::{ConnectorRegistry, SyncConnector};
        let conn = Arc::new(make_connector());
        let reg = ConnectorRegistry::new();
        reg.register("cal", conn.clone() as Arc<dyn SyncConnector>)
            .unwrap();
        reg.connect("cal").unwrap();
        assert_eq!(reg.state("cal"), Some(ConnectorState::Connected));
        let s = reg.sync("cal").unwrap();
        assert_eq!(s.provider, "caldav");
    }
}
