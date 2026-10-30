# bravo-DK-CALENDAR-04-UI — 周/年/列表三视图+提醒段回执

**执行**: bravo ｜ 2026-10-30 ｜ 本 commit（UI 六任务——依赖 Leo 原生面 29c2075）

## 交付面（对照开工令六任务）

| 任务 | 状态 | 落点 |
|---|---|---|
| 1 周视图 | ✅ | week-grid 7 列并排（周界=selected 周一;每列实例色块≤5+「+N 更多」;列头点入切日视图;今日列 outline） |
| 2 年视图 | ✅ | year-grid 12 月缩略格（月名+密度点≤6;点月→monthOffset 定位+切月;月视图标题点入年——PRD P01） |
| 3 日程列表 | ✅ | 分组头今天/明天/本周/更晚（本地日期键）;筛选三态（全/待办关联/生日）;搜索框（标题/地点）;关联/生日徽标 |
| 4 视图切换 | ✅ | cal-mode-seg 四态（月/周/日/列表）+年入口（标题点入） |
| 5 提醒段 | ✅ | EventEditorSheet reminders 段:四档勾选（5/30/60 分钟+1 天）+自定义分钟回车+闹钟级 toggle（alarm_level 铃徽标）;存 reminders: ReminderSetting[]（offset_min/alarm_level snake_case 对齐 core;空=null） |
| 6 今日页 | ✅ | TodayView「今日日程」分区（今日实例展开+色点+时刻;点击跳日历选中日——onOpenDate 复用周条联动链） |

## 验证矩阵

- **Playwright 9/9**（repro-cal04.js:周 7 列+今日列+2 块/列头→日/年 12 月+点月回月/
  分组双头+徽标/筛选 1/搜索命中/提醒 30 分+铃+落库往返/今日分区 2 条/零错）
- 回归: 01 6/6 + 02 7/7 + 03 6/6 全绿
- tsc=0;build 10.6MB
- 保存链 refreshReminders 已接（MobileApp——Leo 04 原生面交接就绪 ✓）

## 路径兼容注记

Leo 7a1e415 将 02/03 脚本 goto 改为 repos/ 路径——本机 dist 在旧路径。
BASE 常量双路径 existsSync 探测（两环境皆可跑——不改 Leo 语义只加兜底）。

## 边界遵守

- ❌ 全屏闹钟 Activity 未做（Receiver 通知够用——P2）
- ❌ 订阅/发现板块未做（红线）
- ❌ Task due 原生提醒未做（Leo 下批独立卡）

——bravo 10-30 23:0x
