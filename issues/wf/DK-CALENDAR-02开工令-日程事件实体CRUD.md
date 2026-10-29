# DK-CALENDAR-02 开工令 · 日程事件实体+CRUD（核）

**派发**: Leo 2026-10-29 22:08 ｜ **Assignee**: Bravo ｜ **Estimate**: 3-5 天 ｜ **Labels**: `area/entity`, `area/mobile-calendar`, `P1`, 依赖: CALENDAR-01 完成（日视图骨架在位）

## 背景

用户裁决（2026-10-29）: Aurora 定位不变——Event 是**笔记生态一等公民**（与 Task/Note 同层），非独立日历模块。评估矩阵置信 75%: 模式熟（Task/Note 先例），新域=时间建模。

## 任务

1. **Event 实体**（entity crate，Task 同模式）:
   ```rust
   pub struct Event {
     pub id: String,
     pub title: String,
     pub start: DateTimeUtc,     // 2026-10-30T09:00
     pub end: Option<DateTimeUtc>,
     pub all_day: bool,
     pub location: Option<String>,
     pub note: Option<String>,   // 关联笔记 id（生态联动点）
     pub color: u8,              // 0-5 标签色
     pub repeat: Option<RepeatRule>,  // 03 卡字段先占位
   }
   ```
2. **存储+FFI**: mobile-ffi 桥 command 五件套（create/list/update/delete/query_by_range）——照 Task 桥模式
3. **CRUD UI**: 月历日格显示当日事件角标（色点条）; 日视图时间轴事件块（可点→编辑 sheet）; FAB 捕获 flow 加「日程」类型
4. **聚合共存**: 日视图时间轴统一排序（事件/任务/日记混排（事件=色块（任务=勾选行

## DoD

- [ ] core 单测: CRUD+按范围查询（含跨月边界
- [ ] 移动断言: 新建事件→月历角标→日视图块→编辑→删除全链路 Playwright
- [ ] 事件与笔记关联（note_id 跳转复用 onOpenNote
- [ ] CI 五门绿+FFI arm64 交叉编译过
