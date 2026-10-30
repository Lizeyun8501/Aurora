# DK-CALENDAR-04-UI 开工令 · 周/年/列表三视图+提醒段（Bravo）

**派发**: Leo 2026-10-30 22:5x ｜ **Assignee**: Bravo ｜ **Estimate**: 2-3 天 ｜ **Labels**: `area/mobile-calendar`, `P0` ｜ 依赖: 04 原生面已落（Leo 今夜——reminders 字段全链+ReminderScheduler 交接就绪）

## 用户裁决（2026-10-30 21:59——本卡总纲）

**方案 B**：Aurora 日历模块 Pro（保持笔记+待办（日历主承载**待办/日程**。提醒升 P0（Task due 同批。订阅/分享维持红线出局（OriginOS 需求八的订阅板块**不做**。

## 任务（P0 补全——PRD 第四章 P0 差额面）

1. **周视图**: 横向 7 天+纵向时间轴（08-22 复用 dayAxis 逻辑泛化——单日→7 日并排;某日点入切日视图
2. **年视图**: 12 月缩略格（月名+当日点+事件点密度微标——点月进月视图
3. **日程列表页**: 全量按时间序（分组头「今天/明天/本周/更晚」;筛选（全/待办关联/日程/生日）+搜索框（标题/地点）
4. **视图切换**: cal-mode-seg 三态→四态（月/周/日/列表+年（月视图点年月标题进年——PRD P01）
5. **EventEditorSheet 提醒段**: 多档勾选（5/30/60 分钟+1 天+自定义分钟;闹钟级 toggle（alarm_level）;存 `reminders: ReminderSetting[]`（adapter 类型已导出——字段名 offset_min/alarm_level snake_case 对齐 core）
6. **今日备忘增强**（今日页联动）: 今日事件块进今日页「日程」分区（Event 按 start 落（与 Task 并列——PRD AI 节「今日备忘」的 P0 简化版: 忙闲概览+重点列表

## 边界（不要做）

- ❌ 全屏闹钟 Activity（Receiver 通知已够——P2 卡再议
- ❌ 订阅/发现板块（红线
- ❌ Task due 原生提醒（Leo 原生面下批——Task 存 localStorage 进不了原生调度（需 Task 桥化——独立卡
- ✅ 走 platform 现有四命令+refreshReminders()（保存链已接 MobileApp

## DoD

- [ ] 周视图渲染+点击日切日视图（Playwright 断言
- [ ] 年视图 12 月+点月切月（断言
- [ ] 列表页分组+筛选+搜索（断言
- [ ] sheet 提醒段三档勾选+闹钟级（mock 断言存 reminders 字段往返
- [ ] 今日页事件分区（断言
- [ ] repro-cal04.js 10+ 断言全绿+tsc 0+build 过+CALENDAR-01/02/03 回归全绿
