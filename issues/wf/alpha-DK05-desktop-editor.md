# Alpha 任务书：DK-05 桌面块编辑器（EditorPane → 共享层 DocumentEditor，切片排期）

> 发起：Bravo · 2026-09-25（对 bravo-DK0F-editor-taskbooks.md 申请 2 的裁决产出）
> 执行：Alpha · 状态：**S1 完成（9/9 PASS），S2 块编辑操作解锁**（S0 报告 docs/DK-05-desktop-input-verify.md）
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

---

## Alpha 回执：S0 完成（2026-09-25 · 3200d5e）

**10/10 PASS，无阻塞性缺陷，S1 解锁。** 报告全文：`docs/DK-05-desktop-input-verify.md`。

- 五场景：焦点/逐键、快速连打（delay:0 60 字符）、IME composition、**滚轮并发输入（桌面差异项）**、长文档滚动光标恢复——全过。
- 附带收益：共享层实体（@aurora/ui-components）在桌面构建栈（vite alias）解析运行全通——S1 接入无实体层风险。
- 风险清单（非阻塞）：Tauri 三真机引擎（WKWebView/WebView2/WebKitGTK）IME/滚轮差异 → S1 只读接入后随打包冒烟；性能采样 → S2 可编辑态一并做。
- lab 基建：`apps/desktop/editor-lab.html` + `vite.config.editorlab.ts` + `scripts/dk05_desktop_verify.js`（S1/S2 迭代复用）。

— Alpha 2026-09-25

---

## Alpha 回执：S1 完成（2026-09-26 · 接入骨架/只读渲染）

**验收全过：desktop tsc + vite build 绿 + 全节点块级渲染正确（19 类对照表全中）+ a11y A 项落地。** 验证脚本 `scripts/dk05_s1_verify.js` 9/9 PASS。

**交付面**：
- `DesktopShell.tsx`：EditorPane `<pre>` → `ReadOnlyAuroraEditor`（共享层实体懒加载挂载；纯文本按行拆段落经 PM 事务灌入，LoroSync 自动同步 loro——S3 双向绑定反向预演；`editable() => false` 只读锁定，S2 摘除即编辑态）。
- a11y（R-04 A 项·正文区）：`role=document` + `aria-label=笔记正文（只读预览）` + `tabIndex=0` 焦点可达 + focus 焦点环 2px（tokens.a11y）。
- 配套：desktop `vite.config.ts` 加共享层 alias + esnext（loro wasm top-level await）+ wasm data URL 内联（3.2MB）+ `server.fs.allow` 仓库根；package.json 补 prosemirror/loro 依赖（对齐 mobile）。
- 顺带清理：RichEditor FloatingMenu 死代码（tick/setTick 未用——desktop tsc 把共享层拉入范围暴露）。
- editor-lab 扩展：`loadFullSchemaDoc()`（schema 全类型 fixture 各一实例）+ `setEditable()` 开关。

**执行中发现并修复的既有缺陷**（S1 之外的真实 bug）：
1. **Tauri IPC 探测缺陷**：`@tauri-apps/api` 包在纯浏览器也可 import 成功，但 invoke 底层依赖 `window.__TAURI_INTERNALS__`——原探测漏宿主检查，browser-mock 回落从未真正生效（调用期抛错整树白屏）。已修：宿主标志同时检查。browser-mock 模式首次真跑通。
2. **dev 模式 wasm 限制备忘**：vite dev 不支持 loro 的 wasm ESM import（需 vite-plugin-wasm，未引入）；验证走 build 产物 `vite preview`（= Tauri 生产静态加载形态，wasm 内联已配）。若后续要 dev 模式调试编辑器再加插件。

**风险/备忘**：a11y 探针基于 Playwright 合成焦点（真实 Tauri WebView 焦点链随 S2 打包冒烟）；AttachmentStrip 与编辑器段落并存（attachment:// 文本行照常渲染，S2 块编辑时统一 embed 化）。

— Alpha 2026-09-26
