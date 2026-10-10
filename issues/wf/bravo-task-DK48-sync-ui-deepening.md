# Bravo Task：DK-48 桌面同步体验深化（UI 三组件）

> 发起：Alpha（TA 指令：UI 桌面同步体验）· 2026-10-11
> 接收：Bravo
> 依据：收口报告候选序第 2 位（用户可感价值最大）；DK-44 已交付三块**数据面+命令面板消费**，本卡把"开发者式命令面板体验"升级为"常驻可视面板体验"。
> 状态：**开工令生效**。直推 main，CI 五门全绿后 Alpha 验收。

## 0. 现状勘察（Alpha 已完成，Bravo 可直接引用）

- **数据面完备（8 command，本卡 Rust 零变更）**：
  - `cmd_sync_status` → SyncStatusDto（gate/wifi_only/queue_len/pending_artifacts+pending_semantic）
  - `cmd_queue_list` → QueueItemDto 明细；`cmd_queue_drain` → DrainReportDto（全量）
  - `cmd_conflict_list` → ConflictDto（kind/id/doc_id/summary=remote_path/created_at）；`cmd_conflict_resolve`（副本留存）/ `cmd_conflict_resolve_semantic`（最新写入优先）
  - `cmd_get/set_wifi_only`
- **消费现状**：全部藏在 CommandPalette 条目里（"手动触发补发"/"冲突处理（待处理 N）"/"解决最早一条"）——功能通但盲选、不可常驻、不可浏览。
- **壳现状**：DesktopShell.tsx 2317 行巨型单文件。**本卡新 UI 一律独立组件文件，禁止往壳里堆行数**。
- ConflictDto **无内容级 diff 字段**（仅路径+时间）——内容对比视图本卡不做（见 §4 停手边界）。

## 1. 三块 UI 组件（独立文件，挂接 DesktopShell）

### 组件 1：SyncStatusBadge（常驻同步指示）—— components/SyncStatusBadge.tsx

- 常驻于 DesktopShell 顶栏/侧栏现有布局缝隙（**不改整体布局骨架**）
- 展示：gate 三态图标（offline/disconnected/wifi 等现有 gate 值）+ `queue_len` 徽标 + 冲突 pending 数红点（pending_artifacts+pending_semantic > 0 时）
- 点击弹出详情浮层：状态明细 + 三快捷入口（打开队列面板 / 打开冲突面板 / wifi_only 开关——复用 cmd_set_wifi_only）
- 刷新：`setInterval` 轮询 cmd_sync_status（15s）+ 组件挂载即刷；**不做事件订阅**（EventBus 桥接超范围）

### 组件 2：QueuePanel（队列可视化）—— components/QueuePanel.tsx

- `cmd_queue_list` 条目表格（路径/时间/现有字段全展示）
- 操作：全量补发按钮（cmd_queue_drain）+ **DrainReportDto 结果报告展示**（成功/失败计数，失败条目列出）
- 空态文案（"队列空——全部已同步"）

### 组件 3：ConflictReviewPanel（冲突逐条决策）—— components/ConflictReviewPanel.tsx

- **逐条决策**替代盲选：列表展示 ConflictDto 全字段（路径/时间/id 短码），每条两个决策按钮——「副本留存」（cmd_conflict_resolve）/「最新写入优先」（cmd_conflict_resolve_semantic，需其要求的参数照实传）
- 决策后即时刷新列表（局部更新，不整页 reload）
- 批量区保留一个「全部按最新写入优先」快捷操作（沿用现有 resolve 语义，逐条调用）
- 空态文案（"无待处理冲突"）

### 挂接与保留

- DesktopShell 挂接：badge 常驻 + 两面板以现有浮层/模态模式打开（跟随壳内既有 modal/panel 惯例，不新造交互体系）
- **CommandPalette 既有 sync 条目全部保留**（深化不替换）
- 样式走 apps/desktop/src/design 现有体系（tokens/变量，不引新 UI 依赖）

## 2. DoD

1. **badge 常驻可见**：gate/queue_len/冲突红点三信息可从主界面直接读出，无需打开命令面板
2. **队列可浏览**：items 明细可见 + drain 结果报告展示（失败条目不静默）
3. **冲突逐条决策**：每条可选保留策略，决策后列表即时更新；空态文案正确
4. **零 Rust 变更**：diff 中 apps/desktop/src-tauri 与 crates/ 全部无改动（发现缺数据面 → 停手回报，见 §4）
5. **壳不膨胀**：DesktopShell.tsx 净增行数 ≤ 40（仅挂接/状态提升）；三组件独立文件通过 desktop-check
6. CI 五门全绿（Test/MSRV/Clippy/Rustfmt/desktop-check）

## 3. 解禁 / 禁触

**解禁**：apps/desktop/src/components/（新增三组件 + DesktopShell 挂接）、design 体系内样式文件。
**禁触**：
- apps/desktop/src-tauri/** 与 crates/**（**全 Rust 面**——纯前端卡）
- CommandPalette 既有条目语义（可复用其 invoke 封装，不删条目）
- ImportWizard / CanvasView 等其余组件

## 4. 停手边界（发现即记录，本卡以现有 command 兑现）

以下缺口**不在本卡**，发现需要即停手回报（另立卡）：
- 冲突内容级 diff 展示（需扩 ConflictDto：local/remote 值或规模摘要）
- 单条队列 drain / 单条重试（cmd_queue_drain 仅全量）
- 同步事件实时推送（EventBus → 前端订阅桥）
- 移动端同款 UI（本卡桌面 only）

## 5. 规模

M（三独立组件 + 挂接；数据面零改，风险收敛在前端装配层）。
