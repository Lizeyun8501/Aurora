# Bravo 任务书 — DK-05M 移动端块编辑器（Android WebView 面）

> 派发对象：Bravo 工作流 agent
> 生成：2026-09-27 · Repo: 本仓 main · 基线 commit: 8701482
> 派发人：Alpha（集成复核方）· 用户指令「派发任务书」
> 优先级：**P0**（V26 P0 最后一卡）· 实际 Estimate: **12–18 人日**（清单卡 65 人日为含 ProseMirror 落地全量口径；editor-uplift 后 ProseMirror 移动端已在跑，本卡为剩余专属面，诚实化口径）

---

## 1. 背景：地基已备，本卡收专属面

DK-0F（editor-uplift）已完成：三件套上移 `shared/ui-components/src/editors/`、mobile 真实 import（@tiptap 源码=0）、`RichEditor` 已在 mobile 挂载。**本卡不做编辑器内核，只做移动专属面**：键盘协同、浮动菜单、安全区、IME 稳定、滚动性能、无障碍。

## 2. 现状锚点（Alpha 实地审计 2026-09-27）

| 锚点 | 现状 | 缺口 |
|---|---|---|
| `apps/mobile/src/MobileApp.tsx` L1500 | RichEditor 挂载（noteId/platform/fallbackText/onDirty/onSaved） | 无键盘协同 |
| `apps/mobile/src/MobileApp.tsx` L7-8 | RichEditor lazy import ✓ | — |
| `apps/mobile/src/styles/mobile.css` L451/L828 | `.editor-toolbar` 为**文档流内 flex 块**（`flex: 0 0 auto`） | 键盘弹起即被盖 |
| 全 mobile src + adapters | `visualViewport`/`virtualKeyboard`/`keyboard` **零引用** | 键盘感知完全缺失 |
| `apps/mobile/android/app/src/main/AndroidManifest.xml` | **未配置** `windowSoftInputMode` | WebView viewport 行为不可靠 |
| `shared/ui-components/src/editors/RichEditor.tsx` | 工具条骨架已有（aria-label「格式工具栏」「选中快捷格式」） | 定位/覆盖面待收 |

## 3. 方案裁决（Alpha 定，Bravo 执行；偏离需回执申请）

1. **键盘感知唯一方案 = visualViewport API**：WebView 无原生键盘高度回调，禁用第三方 keyboard 包。TS 侧监听 `visualViewport.resize/scroll`，将 `visualViewport.height` 同步至 CSS 变量 `--vvh`（rAF 节流）；Android 侧 `MainActivity` 配 `android:windowSoftInputMode="adjustResize"`（viewport 收缩是 visualViewport 事件可靠触发的前置）。
2. **工具条定位**：`position: fixed` 贴 `--vvh` 视口底 + `env(safe-area-inset-bottom)`；键盘弹起=自然悬于键盘上方，收起=贴屏底安全区。**外层容器 `height: var(--vvh)`** 驱动，不依赖 `100vh`（WebView 键盘场景 100vh 不可信）。
3. **选中浮动菜单**：ProseMirror 选区变化 → 坐标计算 + visualViewport 修正 + rAF 节流；选区在键盘遮挡区时浮层贴工具条上方。禁用第三方 popper 库（依赖面）。
4. **IME 候选期光标不跳**：composing 期间抑制 `scrollToSelection` 强滚与编辑器容器重排（复用 DK-05 S0 桌面 IME 时序经验：compositionstart/end 门控）；工具条按钮 `onMouseDown` 必须 `preventDefault`（防编辑器失焦致 IME 中断——桌面侧同因教训）。
5. **安全区**：沿用 `env(safe-area-inset-*)` 既有基建（mobile.css L78/L454），键盘态与静置态分别适配，禁止全局 viewport-fit 改动波及非编辑页。
6. **性能 50fps**：优化手段 Bravo 自主（候选：`content-visibility` / 选区重绘范围收敛）；禁止引入虚拟滚动新依赖。
7. **无障碍**：工具条按钮可 Tab 聚焦、role/aria 完整（既有 aria-label 基线不回退）；**TalkBack 真机遍历标注挂起**（随 DK-17 桌面触点真机冒烟兑现，诚实化条款——Playwright aria 快照为 CI 期替代验证）。

## 4. 验收 DoD（可机器验证，dk05m_verify.js）

- [ ] Playwright mobile viewport（375×667, hasTouch）file:// 加载 dist：
  - **工具条键盘态**：mock visualViewport.height 收缩 → 断言工具条 boundingBox 完整可见（不与键盘区重叠）
  - **IME 稳定**：compositionstart/update/end 注入 → 断言选区 anchor 不漂移、无强制滚动
  - **浮动菜单**：选中文字 → 浮层在可见区；选区入键盘遮挡区 → 浮层贴工具条上方
  - **万字 ≥50fps**：万字文档滚动 rAF 间隔采样 P95 ≤ 20ms
  - **无障碍**：工具条按钮 aria 快照完整可聚焦
- [ ] `npx tsc --noEmit` + `vite build` 绿（mobile）+ desktop tsc 绿（共享层改动回波）
- [ ] AndroidManifest `adjustResize` 配置 + APK 出包冒烟（现成 NDK/SDK 链）
- [ ] schema/既有 mobile 测试零回退（editor-uplift 12/12 基线）
- [ ] 全链 rg 断言：无 `100vh` 键盘态用法、无第三方键盘/浮动库 import

## 5. 半接入警告（D1/DK-16 教训，必读）

- **执行顺序审计**：新 visualViewport 监听若注册在既有 resize/layout 逻辑之后，须确认无短路（DK-16 S1/S2/S3 旧预检先于新前置执行的教训）；
- **一切「已实现」断言在无缓存验证路径成立**（DK-16 交付曾靠 CI 背书闭环——本卡 CI + dk05m_verify 双门）；
- `rg`/`sed` 显示层不可作字节级证据（本仓 L1404 显示吞 `[m` 假象案）——多字节疑点用 python `readlines()+count()` 判真。

## 6. 工作流规约

1. 直接 main 提交，前缀 `feat(DK-05M): ...`；
2. 本地验证铁律：mobile tsc+vite+dk05m_verify 全绿才可 push；CI 五 job 门禁绿；
3. 交付回执写入 `issues/wf/bravo-DK05M-交付报告.md`（含：visualViewport 方案落地说明、DoD 五项逐项证据、挂起项显式标注、CI Run 号）；
4. 回执 push 后通知 Alpha 复核；复核通过前卡状态=待验收。

— Alpha 派发 2026-09-27
