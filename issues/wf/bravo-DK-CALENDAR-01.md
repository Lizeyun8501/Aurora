# DK-CALENDAR-01 — 实现回执

**执行**: bravo ｜ 2026-10-29 23:00 ｜ commits: f9c078b（拆分）+ 本功能 commit

## 交付面

| 任务 | 状态 | 落点 |
|---|---|---|
| 1 黄历宜忌渲染 | ✅（Alpha 295f3a1 已落） | MobileApp.tsx（selLunarInfo 区）——本卡验证确认 |
| 2 月/日双视图+时间轴 | ✅ | views/CalendarView.tsx（mode segmented+dayAxis 聚合） |
| 3 美观渲染精修 | ✅ | mobile.css（按压 scale 0.96/翻月 calAxisIn 0.2s key 重挂/暗色 fest+workday 亮度抬升） |
| 4 CalendarView 拆分 | ✅ f9c078b 独立 commit | src/views/CalendarView.tsx（纯搬移+MobileApp 7 导出） |

## 日视图语义注记

- 时间轴 08-22 十五时段；**任务（due 无时刻）→全天区**；轴外聚合入「更早/更晚」兜底
- 聚合数据源=selData（dayTasks/dayDiary/dayNotes）——**P0 展示聚合**：
  NoteSummary 无 created_at（日记/笔记按 updated_at 本地小时落轴）——事件实体
  真时间 02 卡引入（诚实边界）
- updated_at=RFC3339 UTC → `new Date(iso).getHours()` 本地时区解析（invalid 兜底 -1）

## 验证矩阵

- Playwright（repro-cal01.js 零桥 dist 环境）**6/6**：月历渲染/黄历宜/忌/
  日视图时间轴 08:00 起槽/空日 EmptyState/月回切+翻月/双监听 0 错
- tsc RC=0；主 vite build 10.6MB
- 暗色模式：fest→#F08A8A / workday→#EAC36A（亮度抬升可读性）——CSS 变量主体自动切换

## 坑实录

- **root 属主文件三犯**：Write 工具产物 root 属主，python 写回 Permission denied
  → rm 夺回重建（10-15 ⑧ 纪律第三次实证——Write 后如需脚本级 patch 先 chown 探测）
- 拆分搬移用 `git show HEAD:file | sed -n` 原文提取（防 Read 工具行首伪影——
  本轮 Read 输出曾吞 `[mo` 前缀字符）

——bravo 23:00
