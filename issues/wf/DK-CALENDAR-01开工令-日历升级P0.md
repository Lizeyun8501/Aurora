# DK-CALENDAR-01 开工令 · 日历升级 P0（形+渲染层）

**派发**: Leo 2026-10-29 22:08（用户裁决:评估有条件通过——**Aurora 定位不变（笔记 app），本轮=升级日历功能与界面美观渲染，非日历 app 化**） ｜ **Assignee**: Bravo ｜ **Estimate**: 1-1.5 天 ｜ **Labels**: `area/mobile-calendar`, `P0`, `ui`

## 定位红线（用户 2026-10-29 22:08 明示）

- Aurora 仍是笔记应用——日历是功能模块，不做独立日历 app
- **不做**: 天气、ics 订阅、日程分享/邀请（评估矩阵「不建议」项全部出局）
- 视觉归 V25 设计语言，不引入新体系

## 任务

1. **黄历宜忌渲染**：CalendarView 当日详情头部（selLunarInfo 区）下加黄历行——`lunar.getDayYi()/getDayJi()` 各取前 4 项，样式 `.almanac`（宜/忌分色微标，暗色模式同过）
2. **日视图**：月/日双视图 segmented 切换（cal-head 右侧）。日视图=时间轴列（08-22 时段条+当日已有聚合落轴：任务（due 日）/日记（创建时间）/笔记（更新时间当日）——P0 只做**展示聚合**（不做事件实体（那是 02 卡）
3. **美观渲染精修**：
   - 月历今日/选中态动效（scale 0.96→1 按压+transition 0.18s）
   - 月份翻页 transition（月格 opacity/translateY 0.2s，无 FLIP 重排风险时启用）
   - 节假日红/调休灰与暗色模式全对过
4. **架构**: CalendarView 从 MobileApp.tsx 拆出 `src/views/CalendarView.tsx`（MobileApp 已 2000+ 行——纯搬移零逻辑改动，单独 commit）

## DoD

- [ ] 选中任一日显示「宜:… 忌:…」行（Playwright 文本断言（2026-10-29 实测）
- [ ] 月⇄日切换可达且日视图时间轴有当日聚合（空日显示 EmptyState
- [ ] 暗色模式截图对比零回归（月历+周条+日视图三视图
- [ ] tsc+build 绿+CI 五门绿
- [ ] Playwright 行为断言: 周条联动→日视图选中一致
