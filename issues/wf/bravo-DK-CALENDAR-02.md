# bravo-DK-CALENDAR-02 — 实现回执

**执行**: bravo ｜ 2026-10-30 ｜ commits: 本 commit（Java 桥+TS adapter+前端 UI）；
Rust 面（entity/event_store/ffi/JNI export）= Alpha db5c981

## 交付面（对照开工令）

| 任务 | 状态 | 落点 |
|---|---|---|
| 1 Event 实体 | ✅ db5c981 | entity（Event/RepeatRule/RepeatFreq——03 卡 repeat 冻结占位） |
| 2 存储+FFI 桥 command | ✅ 本 commit | UniffiAppCore.java EventItem 段+四 native 声明；MainActivity @JavascriptInterface 四方法（JSON 串 searchNotes 惯例）；TS adapter 八方法（interface+封装+mock localStorage——本批同时补 DK02 回收站 TS 桥面 4 条） |
| 3 CRUD UI | ✅ 本 commit | 月历日格事件色点角标（3 色上限）；日视图时间轴事件色块（all_day→全天区;时刻事件按 start 本地小时;轴外入兜底区）；EventEditorSheet（标题/datetime-local 起止/全天/色板 0-5/地点/关联笔记跳转/删除 confirm 门）；「为这一天添加日程」入口；FAB 捕获流「存为日程」类型（今日 09:00 预填+captureDue 优先） |
| 4 聚合共存 | ✅ | dayAxis 混排（event=色块/task/note/diary——色块 inline rgba 15% 底+实色字） |

## 验证矩阵

- **Playwright 全链 7/7**（repro-cal02.js——mock localStorage 路径）：
  新建→落库 1 条→月历 today 色点→日视图事件块→编辑往返（title 改动持久）→
  删除→落库 0 条+角标消失→双监听 0 错
- CALENDAR-01 回归 6/6（共享组件大改后无砸）
- tsc=0；vite build 10.6MB
- CI 五门+arm64 交叉编译/gradle：push 后终证（Java/TS 面编译链）
- core 单测：db5c981 自述 crud/range 边界/fail-closed/repeat 往返在案

## 时区纪律（Leo 8f82772 教训内化）

所有日界/小时换算统一 Date 本地解析：eventLocalKey（月格含跨天事件 sKey≤key≤eKey）/
dayAxis hour/EventEditorSheet toLocalInput（RFC3339→datetime-local 双向）/
upsert 存 ISO UTC。mock queryEvents 同 core 交叠语义（start<to && (end|start)>from）。

## 已知边界（诚实注记）

- 事件编辑 sheet 的 note_id 关联：P0 手填场景未开 UI（sheet 显示「打开关联笔记」
  仅当 ev.note_id 已有——2.x 卡接选笔记器）
- FAB「存为日程」默认色 1（蓝）——颜色后置 sheet 调
- 真桥 arm64/gradle 链：本机无 NDK 面——CI APK 流水线终证

——bravo 10-30 08:5x
