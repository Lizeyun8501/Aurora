# Bravo Task：DK-50 移动端同步三组件移植（DK-49 已解锁）

> 发起：Alpha（TA 拍板：发）· 2026-10-11 20:22
> 接收：Bravo
> 依据：DK-49 数据面已关账（2cea82c+04868d1，adapters 7 调用面就绪）；本卡=桌面 DK-48 三组件（461c0c6）移植到移动端，收口"移动端同款"。
> 状态：**开工令生效**。直推 main，CI 五门全绿后 Alpha 验收。

## 0. Alpha 勘察（移植适配点，Bravo 自行复核）

**挂接点（MobileApp.tsx 2071 行，红线见 §2/5）**：
- badge → `app-bar`（L467 header）内，不破现有布局
- 两面板 → 移动端现成 fixed overlay 惯例（L1482 全屏遮罩 zIndex 65 级别，全屏 sheet 形态优于桌面浮层）

**样式体系差异（不可直搬）**：
- 桌面组件 `import tokens from '../design/tokens'`；移动端=styles/mobile.css 单文件类名体系，**无 tokens 模块**
- 移植裁决：逻辑层（轮询/数据流/分型决策/联动）照搬，样式层改写为 mobile.css 体系（新类名 `sync-*` 前缀，进 mobile.css，不引新依赖不建 tokens）

**数据源差异**：
- 桌面组件收 `invoke` prop；移动端=DK-49 `platform.syncStatus()/queueList()/queueDrain()/conflictList()/conflictResolve()/conflictResolveSemantic()/getWifiOnly()/setWifiOnly()`（androidPlatform，mock 录制回放已就绪）
- 组件 props 改传 platform 对象（或适配包装），**不直调 window.AndroidBridge**

## 1. 三组件移植（apps/mobile/src/components/ 新建目录）

### SyncStatusBadge → app-bar 常驻

- gate 三态图标+文字+语义色三通道（桌面无障碍裁决延续）；queue_len 徽标；冲突红点（pending 双口径）
- 点开=全屏 sheet 详情（状态明细+三快捷入口：队列/冲突/wifi 开关）
- **功耗裁决**：15s 轮询照桌面，但 `document.visibilitychange` 不可见即暂停、恢复可见即刷——移动 WebView 切后台不空转

### QueuePanel → 全屏 sheet

- queueList 明细+queueDrain 按钮+DrainReport 展示（**failed_docs 逐条列出，失败不静默**）+空态文案；对齐桌面语义

### ConflictReviewPanel → 全屏 sheet

- 分型分接口逐条决策（artifact→conflictResolve / 语义→conflictResolveSemantic）；**artifact 不适用 LWW 置灰**（桌面 DK-48 亮点 1 延续）；决策后 onChanged 联动 badge 即刷；批量区 artifact 跳过并计数；空态文案

### 挂接纪律

- MobileApp.tsx 净增 ≤40 行（仅 import+state+挂接）；其余全在组件文件
- 触发入口：badge 为主；MobileApp 现有菜单（menuOpen 侧单）加同步入口两条，遵循其既有条目写法

## 2. DoD

1. badge 常驻 app-bar 可见（gate/queue/冲突红点），sheet 详情+三快捷入口可用
2. 队列可浏览+drain 报告 failed_docs 展示；冲突逐条决策+置灰防御+即时刷新
3. **visibilitychange 功耗裁决落地**（不可见暂停轮询）
4. 数据面零 Rust 变更：diff 中 crates/** 零文件（本卡纯前端+样式）
5. **MobileApp.tsx 净增 ≤40**；组件文件全在 apps/mobile/src/components/（新建）；样式进 mobile.css `sync-*` 前缀
6. mock 回放路径可用（无 AndroidBridge 环境下三组件空态/降级正常）
7. CI 五门全绿（desktop-check 覆盖移动前端 TS 编译——若移动前端有独立 check 则对齐）

## 3. 解禁 / 禁触

**解禁**：apps/mobile/src/components/（新建）、apps/mobile/src/styles/mobile.css、apps/mobile/src/adapters/androidPlatform.ts（仅类型/签名微调，如需）、MobileApp.tsx（≤40 行挂接）。
**禁触**：
- crates/** 与 apps/mobile/android/**（DK-49 数据面已就绪，本卡零触碰）
- apps/desktop/**（桌面三组件为移植源，只读）
- MobileApp 既有业务逻辑（笔记 CRUD/复习队列/编辑器）
- bridge.ts 与 Capacitor 插件面

## 4. 停手边界

- platform 签名与组件所需不符（如缺字段）→ 用现有字段如实降级展示，缺什么记录，**禁止回头改 mobile-ffi**
- app-bar 空间不足以放 badge（窄屏）→ 移入 menuOpen 侧单顶部，记录裁决
- 移动端无 modal/sheet 通用组件需自建 → 允许轻量自建（fixed overlay 照 L1482 惯例），不引 UI 库
- 任何需要动数据面/桥层的情况 → 停手回报

## 5. 规模

M（三组件移植+样式改写+挂接；逻辑层照搬风险低，工作量在样式适配与移动交互形态）。
