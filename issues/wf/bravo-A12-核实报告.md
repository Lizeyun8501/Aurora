# A-12 核实报告：每日笔记「自动创建 + 作为捕获落地页」

> 执行: Bravo ｜ 工单: TA 2026-11-14 08:13 ｜ 性质: 核实卡（两问缺口已补最小实现）｜ commit: 本档同链

## 核实结论（三问·代码锚点级）

### ① 首次打开「今日」是否自动创建当日笔记？
**核实前实况: 后端全备/前端按钮驱动** → **已补（最小实现）**
- 后端（**已具备**,无需改）:
  - `crates/aurora-core/src/write_path/daily.rs:111` `ensure_daily_note`——幂等键=`daily_note_lock` 互斥 + KV `daily_key(date)` 定位（同日重复调用返回同一篇,零重复）;Auto 建/Manual 仅定位/Off 拒
  - `apps/desktop/src-tauri/src/lib.rs` `cmd_daily_note_open`——ensure 通道（WritePath 正门,含 seal/unseal 加密对）
- 前端（**缺口,已补）**: 原 `TodayDailyNote.tsx` 仅按钮点击才 ensure（`openToday`）——非「视图进入自动」
- **补**: `apps/desktop/src/components/TodayDailyNote.tsx`——Auto 模式 useEffect 进视图自动 ensure（不跳转——备好态 `readyId`;按钮文案三态「已备好——打开/打开今日笔记/定位/创建」）;Manual 不自动建（模式语义尊重）;Off 静默
- **幂等单测在案实跑**: `crates/aurora-bootstrap/tests/dk19_daily.rs` **5/5 绿**（本机 RC=0）——`daily_note_survives_restart`（重启后 created=false+同 note_id）+ **`concurrent_ensure_creates_single_daily_note`（并发同日单篇——正中 DoD「同日重复调用不产生两篇」）**

### ② Alt+Shift+N 快速捕获落点？
**核实前实况: 落 #capture 页（非 journal）** → **已改（默认 journal+Inbox 可选）**
- 原链: `apps/desktop/src/components/QuickCapture.tsx` `save()`→`cmd_import_share`（新笔记）+`cmd_set_note_tags(['capture'])`——即 10-21 审阅指「落 #capture 标签页」
- **改**: 双通道——默认「落今日笔记」（`cmd_daily_note_open` ensure→`cmd_get_note` 读 content→时间戳引块追加（`> ⚡ [快速捕获] HH:mm`——landShare A-06 惯例格式）→`cmd_update_note` 写回,WritePath 正门）;「捕获到 Inbox」次按钮保留旧链（不破坏既有习惯——工单原文）;Cmd/Ctrl+Enter=默认 daily;Off 模式降级提示（指向 Inbox 钮）
- 移动端 A-06 landShare 已是 daily 语义（`apps/mobile/src/MobileApp.tsx:89`——当日「日记 YYYY-MM-DD」定位/建+追加）——**双端语义现已对齐**

### ③ 今日任务自动汇聚到当日笔记？
**结论: 视图级汇聚已实现——笔记级写入不做（架构裁决,非缺口）**
- 锚点: `apps/desktop/src/components/DesktopShell.tsx` `TodayTaskRows`——`cmd_today_task_rows` 投影实时查询（逾期/今日分区+`cmd_upcoming_tasks` 7 日窗）,今日视图内嵌渲染
- **不写入笔记本体**: 任务事实源=tasks 投影域（WritePath 纪律）;写进 journal 文档=快照漂移双源（任务状态变更后笔记内文本腐化）——视图级汇聚已达成用户可见目标且零腐化
- 判定: V25.1 缺口语义若指「任务写入笔记」→ 设计裁决不做;若指「今日视图可见任务」→ 已实现

## V26 矩阵修订（防再误判）

`issues/v26-未完成盘点-20260928.md` R-03 条目「每日笔记自动创建」列 P1 缺口（09-28 时点 0 commits——**后 DK-19 已落但矩阵未回写**）:
- ✅ 每日笔记自动创建（DK-19: core ensure+模式/模板/跨重启/索引自愈+并发幂等测试）——A-12 补前端 Auto 自动 ensure 后全链闭环
- ✅ 快速捕获落当日 journal（A-12: 默认 daily+Inbox 可选——10-21 审阅建议已落）
- ✅ 今日任务视图级汇聚（TodayTaskRows 投影——笔记级写入=双源腐化,架构裁决不做）
- 原 R-03 七项现况: 附件预览器 A-02/源码模式 A-03/自定义主题 A-11/每日笔记 A-12 均已交付;单篇加密 Vault/任务依赖/时间追踪未核（另卡）

## docs 索引更新
本报告落 `issues/wf/`（工单域惯例）;`docs/` 侧无功能矩阵档（矩阵在 issues/v26-* 两档——修订节已回写其增量档引用面）。

## 停手边界遵守
不做日记模板变量引擎增强（{{date}}/{{weekday}} 既有面未动——归 A-04 后续）✓ / 不做历史日记回填 ✓
