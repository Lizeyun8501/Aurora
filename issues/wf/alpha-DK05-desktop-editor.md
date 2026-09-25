# Alpha 任务书：DK-05 桌面块编辑器（EditorPane → 共享层 DocumentEditor，切片排期）

> 发起：Bravo · 2026-09-25（对 bravo-DK0F-editor-taskbooks.md 申请 2 的裁决产出）
> 执行：Alpha · 状态：**待取件（前置：editor-uplift 卡合入后开工）**
> 依据：ADR-005 任务 4（EditorPane 纯文本预览升级为共享层 DocumentEditor）+ DK-05M-V 报告风险项（loro-prosemirror 桌面输入时序需复验）
> 性质：65 人日大件，按可独立验收切片推进；本书定切分与门槛，逐切片走验收→装配→回执循环。

## 1. 背景与现状锚点

- `apps/desktop/src/components/DesktopShell.tsx`：EditorPane 内嵌（:411 起），正文为纯文本 `<pre>`（:475 附近），元信息 + 标题编辑已有；
- desktop package.json **无 loro-crdt / loro-prosemirror / prosemirror-\***（宿主依赖需本卡引入）；
- 共享层编辑器实体由 editor-uplift 卡产出（本卡前置，无实体不开工）；
- DK-05M-V 真机补验清单 4 项（IME 候选栏组合 / 键盘弹起 resize / WebView 存储清理 / 惯性滚动）——均为 mobile 系统行为；**桌面侧等价物 = 滚轮 + IME 组合输入时序**，需先复验再挂内核。

## 2. 切片排期（每片独立验收）

### S0 — loro-prosemirror 桌面输入时序复验（开工首项，阻塞后续）

- 仿 `dk05mv_verify.js` 思路出桌面复验脚本/清单：滚轮滚动中输入、IME 组合输入（拼音流式）、快速连续输入下 loro-prosemirror 0.4.4 绑定稳定性；
- 产出：`docs/DK-05-desktop-input-verify.md`（结果 + 风险清单）；**发现阻塞性缺陷 → 回执并停线**，不带病挂内核。

### S1 — 接入骨架（只读渲染）

- EditorPane `<pre>` → 共享层编辑器挂载（只读态渲染 loro 文档快照）；
- DesktopShell 布局调整随带（ADR-005 已知代价）；无障碍 A 项随带：编辑区 aria-label / 焦点可达 / 焦点态可见（R-04 A1-A9 中正文区相关项）；
- 验收：desktop tsc + vite build 绿；打开含 schema 全节点类型的笔记，块级渲染正确（对照 schema 合并对照表）。

### S2 — 块编辑操作（可编辑态）

- 编辑态 + 块级操作：标题升降级 / 列表 / task_block 勾选 / code_block / 撤销重做（ProseMirror history）；
- 落库链路：编辑产物序列化走既有 Tauri command（禁绕过）；防抖落库 + 退出前 flush；
- 验收：块操作行为断言（脚本或测试）+ 落库往返一致（存→取→渲染等价）+ a11y 键盘全操作路径。

### S3 — Loro CRDT 绑定

- EditorPane 文档与 Loro doc 双向绑定：本地编辑 → loro txn → 持久化；多端（mobile 编辑后桌面拉取）增量渲染；
- undo/边界：CRDT undo 与编辑器 history 协调策略定型；
- 验收：双端模拟往返断言（mobile 写 → 桌面读改 → mobile 读）；崩溃恢复（重开渲染一致）。

### S4 — 收尾

- 移动端三件套与共享层完全同源后 mobile 侧去重核对（RichEditor 若有 mobile 专属壳保留，逻辑归共享层）；
- 无障碍 A 项全项复核；`vite.config.editorlab.ts` 编辑器实验舱更新指向共享层。

## 3. 边界

| 项 | 归属 |
|---|---|
| 本卡全部 desktop src 改动 + desktop package.json 宿主依赖 | Alpha |
| 共享层编辑器实体/schema 缺陷修复 | editor-uplift 卡遗留 → 本卡可修，回执注明 |
| mobile 侧改动 | 仅 S4 去重核对 |
| Rust 侧新 command/存储接口 | 单独 request 文档（bravo-request-* 若涉 Bravo 领地） |

## 4. 总验收（DK-05 起点达成）

- 桌面正文为 ProseMirror + Loro 绑定编辑器（ADR-005 退出条件第 3 条 ✓）；
- 双端编译门禁绿；CI 全绿；
- 每切片回执：验证矩阵（build / 行为断言 / a11y 项）+ commit 哈希。

## 5. 提交约定

- 分支：`wf/alpha-dk05-desktop-editor`（每切片自最新 origin/main 切出或续推）；
- commit 前缀 `feat(DK-05):` / `test(DK-05):` / `docs(DK-05):`；
- push 前自查 `origin/main..HEAD` 领地合规。

— Bravo 2026-09-25
