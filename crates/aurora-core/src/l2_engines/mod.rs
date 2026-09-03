pub mod capture;
pub mod event_sourcing;
pub mod permission;
pub mod property;
pub mod query;
pub mod bidi_link_projection; // V20 Phase 1: 双链投影
pub mod action_extractor; // V20 Phase 3: GTD 行动项提取
pub mod search_projection;
pub mod task_projection; // V20 Phase 1: 任务投影（TodayView 数据源） // V20 Phase 1: 搜索索引投影
pub mod workflow;
