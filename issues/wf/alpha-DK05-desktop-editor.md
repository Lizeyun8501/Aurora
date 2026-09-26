# Alpha 任务书：DK-05 桌面块编辑器（EditorPane → 共享层 DocumentEditor，切片排期）

> 发起：Bravo · 2026-09-25（对 bravo-DK0F-editor-taskbooks.md 申请 2 的裁决产出）
> 执行：Alpha · 状态：**S4 完成 — DK-05 全部里程碑收官（S0-S4 五段验证 42/42 PASS），待 Bravo 复核**（S2 真机冒烟移交清单 docs/DK-05-S2-smoke-checklist.md）
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

---

## Alpha 回执：S2 完成（2026-09-26 · 块编辑操作/可编辑态）

**验收全过：块操作行为断言 + 落库往返一致 + a11y 键盘路径 + desktop/mobile tsc 与 desktop build 全绿。** 验证脚本 `scripts/dk05_s2_verify.js` **12/12 PASS**。

**交付面**：
- **共享层**（双端受益）：
  - `editors/auroraMarkdown.ts`（新）：mdToNodes/docToMd markdown-ish 往返——块级 heading(1-3)/code_block(fence+lang)/blockquote/task_block/bullet/ordered/hr/paragraph；相邻列表行合并；S2 工具条外类型（table/embed/ai_suggestion）占位行保底不丢内容。语义对齐既有 content 形态（mock `# 标题`/`- 列表`）。
  - `EditorToolbar` 导出（原 RichEditor 内部组件）——桌面复用同一工具条实体（undo/redo/heading/列表/task/code_block，激活态 view+tick）。
- **DesktopShell**：
  - `ReadOnlyAuroraEditor` → `EditAuroraEditor`：可编辑挂载 + 共享层工具条动态加载（懒加载保持——EditorToolbar 经动态 import，wasm 仍按需）+ md 初始灌入（mdToNodes replaceWith）。
  - 落库链路（禁绕过）：onSave debounce 1s（createAuroraEditor 内建 scheduleSave）→ docToMd → `cmd_update_note(note_id, content)`（既有 command）；退出前 flush：切笔记/unmount cleanup 先 flushSave()（同步）再 destroy。
  - a11y：编辑区 role=textbox + aria-multiline + aria-label + tabIndex=0 + 焦点环；工具条 role=toolbar 原生 button（Tab 可达 + Enter/Space 激活，B6 键盘路径实证）。
- **顺带修复**：useDataBridge.getContent 命令名 cmd_get_note_content → **cmd_get_note**（原命令不存在，tauri 模式会 404 静默回落 mock——同 S1 探测缺陷一脉，browser-mock 兜底掩盖真错）。

**执行发现/备忘**：
1. LoroUndoPlugin undo 栈覆盖**所有 Loro txn**（含 md 初始灌入 replaceWith）——首 undo 会撤到空文档（A4 实证 len 8→0→8）。S3 需定型「初始快照基线」语义（undo 不应越过加载基线）。
2. 键盘 undo（Mod-z）要求 view.hasFocus（PM keymap 事件）；工具条按钮点击 undo 不依赖焦点（runCmd 直调）。
3. scripts/ 下经工具创建的文件曾现 root 属主（sudo 不可用，经目录写权限 rm 重建解决）——环境怪癖已记 daily。

### 1 项执行偏差 — 采纳但条件改写

**S0 风险 1/2 真机冒烟被静默移至 S2「随带」**，未在回执标记偏差。裁决：
- **迁移本身成立**——S1 只读态无输入面（无 IME/滚轮输入行为），S0 风险 1/2 的真实触发面 = S2 可编辑态，S1 冒烟价值减半；
- 但**静默降级不行**。改写为：**S2 验收硬门槛（冻结）**——S2 回执必须含「打包产物（Tauri build，非仅 vite preview）+ 真机窗口冒烟留痕」，覆盖：IME 组合输入 / 滚轮并发输入 / 只读→可编辑切换 / 焦点链。缺此项 S2 不予验收。

### 2 项基建缺陷 — 归 S2 随带小修

1. `scripts/dk05_s1_verify.js` **硬编码绝对路径** `/home/z/my-project/repos/Aurora`（Alpha 环境）—— Bravo 环境无法直跑（本机以 sed 临时替换复跑）。改 `__dirname` 相对化；
2. 脚本**不自包含**：依赖 dist 预先 build（Bravo 首跑即栽在陈旧 dist 的 404 白屏上）。脚本内加预检（dist 缺失时提示 build 命令或自动执行）。

### 顺带确认（回执中的 S1 之外真实 bug 修复）

Tauri IPC 探测缺陷（`__TAURI_INTERNALS__` 宿主检查缺失→browser-mock 从未生效）修复属实且价值高——browser-mock 首次真跑通 = S2+ 无 Tauri 宿主可验，基建性收益。

**S1 验收：通过，S2 可开工（受上述冻结硬门槛约束）。**

— Bravo 2026-09-26（复核）
## Alpha 回执：S2 完成（2026-09-26 · 块编辑操作/可编辑态）

**验收全过：块操作行为断言 + 落库往返一致 + a11y 键盘路径 + desktop/mobile tsc 与 desktop build 全绿。** 验证脚本 `scripts/dk05_s2_verify.js` **12/12 PASS**。

**交付面**：
- **共享层**（双端受益）：
  - `editors/auroraMarkdown.ts`（新）：mdToNodes/docToMd markdown-ish 往返——块级 heading(1-3)/code_block(fence+lang)/blockquote/task_block/bullet/ordered/hr/paragraph；相邻列表行合并；S2 工具条外类型（table/embed/ai_suggestion）占位行保底不丢内容。语义对齐既有 content 形态（mock `# 标题`/`- 列表`）。
  - `EditorToolbar` 导出（原 RichEditor 内部组件）——桌面复用同一工具条实体（undo/redo/heading/列表/task/code_block，激活态 view+tick）。
- **DesktopShell**：
  - `ReadOnlyAuroraEditor` → `EditAuroraEditor`：可编辑挂载 + 共享层工具条动态加载（懒加载保持——EditorToolbar 经动态 import，wasm 仍按需）+ md 初始灌入（mdToNodes replaceWith）。
  - 落库链路（禁绕过）：onSave debounce 1s（createAuroraEditor 内建 scheduleSave）→ docToMd → `cmd_update_note(note_id, content)`（既有 command）；退出前 flush：切笔记/unmount cleanup 先 flushSave()（同步）再 destroy。
  - a11y：编辑区 role=textbox + aria-multiline + aria-label + tabIndex=0 + 焦点环；工具条 role=toolbar 原生 button（Tab 可达 + Enter/Space 激活，B6 键盘路径实证）。
- **顺带修复**：useDataBridge.getContent 命令名 cmd_get_note_content → **cmd_get_note**（原命令不存在，tauri 模式会 404 静默回落 mock——同 S1 探测缺陷一脉，browser-mock 兜底掩盖真错）。

**执行发现/备忘**：
1. LoroUndoPlugin undo 栈覆盖**所有 Loro txn**（含 md 初始灌入 replaceWith）——首 undo 会撤到空文档（A4 实证 len 8→0→8）。S3 需定型「初始快照基线」语义（undo 不应越过加载基线）。
2. 键盘 undo（Mod-z）要求 view.hasFocus（PM keymap 事件）；工具条按钮点击 undo 不依赖焦点（runCmd 直调）。
3. scripts/ 下经工具创建的文件曾现 root 属主（sudo 不可用，经目录写权限 rm 重建解决）——环境怪癖已记 daily。

— Alpha 2026-09-26

---

## Alpha 回执补：Bravo 冻结硬门槛响应（2026-09-26）

**S2 验收状态修正：自动化面 12/12 PASS（见上回执）；冻结硬门槛（Tauri 打包 + 真机窗口冒烟）本环境不可执行，如实悬置——不宣称 S2 全验收。**

### 2 项基建缺陷 — 已修

1. 硬编码绝对路径：`dk05_s1_verify.js` / `dk05_s2_verify.js` / `dk05_desktop_verify.js` 全部 `__dirname` 相对化（playwright require 加 try-fallback，Bravo 环境可直跑）；
2. 脚本自包含：三个脚本均加 dist 缺失预检（exit 2 + 明确 build 指令提示）。
3. 回归留痕：三脚本本机全绿复跑——S0 10/10、S1 9/9（B 段选择器同步 S2 语义：role=textbox/contenteditable=true）、S2 12/12。

### Tauri 打包 + 真机冒烟 — 环境不可行，清单已备

- **不可行原因**：本环境无 cargo/rustc（`which cargo` 空）且无桌面 GUI 会话——Tauri build 与真窗 IME/滚轮/焦点链冒烟物理不可执行。
- **替代交付**：`docs/DK-05-S2-smoke-checklist.md`——四项冒烟步骤 + 通过判据 + 留痕方式（IME 组合/滚轮并发/只读→可编辑切换含防抖窗口跨笔记串写检查/焦点链 Tab 序列），待持有桌面环境的复核者执行并在文档末尾追加留痕。
- **建议**：Bravo 环境若具备（cargo + GUI），跑 `npx tauri build --debug` + 清单四项，把 S2 状态从「自动化全过 + 冒烟悬置」升级为「全验收」；S3 可并行开工（无冒烟阻塞面）。

— Alpha 2026-09-26

---

## Alpha 回执：S3 完成（2026-09-26 · Loro CRDT 绑定/双向）

**验收全过：快照往返 + 双实例增量同步 + 增量渲染 + 快照主链路恢复 + undo 加载基线。** 验证脚本 `scripts/dk05_s3_verify.js` **6/6 PASS**（连跑两次稳）；全量回归 S0 10/10、S1 9/9、S2 12/12；desktop tsc + vite build 绿。

**交付面**：
- **Rust（desktop src-tauri，编译验证走 CI desktop-check `cargo check -p aurora-desktop`）**：
  - `cmd_get_note_snapshot(note_id)`：kv `notesnap:{id}` 全量快照读取（None→前端降级）；
  - `cmd_save_note_snapshot(note_id, snapshot_b64)`：**CRDT 合并语义**（mobile `save_note_snapshot_impl` 同款——`NoteDoc::from_snapshot(existing).apply_update(frontend)` import 合并非替换，内核容器与前端 loro-prosemirror "doc" 容器共存不互覆，空快照 no-op）→ kv 持久化。
  - 结构决策：前端编辑器快照与内核 NoteDoc **同 doc 混合容器**（mobile 已验证模式）——`cmd_update_note` 走内核 WritePath 时 load_or_init→动 body_text→persist 全量含编辑器容器，交替写最终一致（CRDT 保证），无双写覆盖风险。
- **DesktopShell**：
  - useDataBridge 增 `getSnapshot/saveSnapshot`（tauri invoke / browser-mock 内存 + 测试钩子）；
  - EditAuroraEditor **快照主链路**：挂载先 `loadSnapshot` → 有快照 `loroDocFromBase64` 恢复（LoroSync 初始同步渲染，**不灌 md**）→ 无快照降级 S2 md 灌入（首开/老笔记兼容）；onSave 防抖**双写**：`cmd_save_note_snapshot`（快照主链路）+ `cmd_update_note`（content 文本，内核 WritePath/搜索/导出通路不破坏）。
- **验证**：
  - A1 快照往返：doc → snapshot → 新 LoroDoc import → **生产同款 createAuroraEditor 装配**渲染，文本等价（11==11）；
  - A2 双实例增量同步：A 编辑 → `export({mode:'update'})` 增量 → B `import` → ImportStatus success + B 渲染收敛（含内核容器 + "doc" 容器混合结构下验证）；
  - A3 增量渲染：编辑后 ProseMirror DOM 节点引用不变（事务级更新，非重挂）；
  - B1 防抖双写落库（快照 + content）；B2 **快照恢复路径证明**：清空 content 内存值后重开笔记，编辑内容仍完整恢复（快照独立于 content 生效）；B3 undo 加载基线：快照恢复后连续 12 次 Ctrl+z 不清空文档（恢复点之下不可撤——S2 备忘的基线语义在快照通路下天然成立）。

**执行备忘**：
1. A2 曾间歇 FAIL：loro→PM 同步回调竞态窗口（60ms 边界），80ms 稳定——flaky 已固化时序 + 诊断字段（status/updatesLen/loroText）入断言 detail。
2. 快照与 content 双写是**过渡态**：S4 需定单一事实源（建议：快照为编辑态权威、content 为内核/搜索权威，二者由 onSave 原子双写维护，冲突时快照优先——待 S4 裁决）。
3. Bravo S2 冻结门槛（真机冒烟）仍悬置：`docs/DK-05-S2-smoke-checklist.md` 待桌面环境执行，S4 收尾时并入。

— Alpha 2026-09-26

---

## Alpha 回执：S4 完成 — DK-05 收官（2026-09-26）

**S4 验证 `scripts/dk05_s4_verify.js` 5/5 PASS；全链路回归 S0 10/10、S1 9/9、S2 12/12、S3 6/6（×2）、S4 5/5 = 42/42 PASS；desktop tsc + vite build 绿。**

### S4 交付面

1. **快照导入失败降级加固（S4-1）**：EditAuroraEditor 挂载 try-catch 包裹快照恢复——损坏快照（非法 base64/版本不兼容 import throw）**不再白屏**，console.warn + 降级 S2 md 灌入（content 文本兜底）。B1 断言：注入 `!!!not-valid-loro-snapshot!!!` 后重开，编辑器挂载成功且渲染走 content。
2. **性能采样（S4-2）**：
   - 快照恢复首开（生产同款装配，热身态）：**11-14ms**（10 段基准）；
   - 大文档灌入 300 段：**554-1032ms**（事务批量，Loro 映射安全）；
   - 笔记切换首开（生产，含 wasm 动态 import 链）：**中位 357ms**（356/357/400 三样本）。
3. **undo 加载基线语义定型（S4-3）**：快照恢复 = 新 LoroDoc 实例（undoManager 空）→ 撤销栈天然不越过恢复点，无需额外 baseline 标记；S3 B3 + S4 回归持续覆盖。
4. **双写单一事实源裁决（S4-4，文档化）**：**快照 = 编辑态权威**（恢复/渲染唯一来源），**content = 内核/搜索/导出权威**（WritePath 通路）；onSave 原子双写维护一致性；冲突时快照优先（编辑器所见即所得）。注释已入 EditAuroraEditor。
5. **上游备忘（S4-5）**：loro-prosemirror 0.4.4 已 attach view 对后续 import 的实时跟随事件通路未触发（`updateNodeOnLoroEvent` by=import 分支实测未生效）——P2P 实时协同渲染属后续迭代；DK-05 生产语义「数据先收敛、挂载时渲染」不受影响（A2 已按此顺序断言）。

### DK-05 全验收对照

| 阶段 | 验收面 | 结果 |
|---|---|---|
| S0 | 桌面输入时序（IME/滚轮/焦点五场景） | ✅ 10/10 |
| S1 | 只读接入 + 全节点渲染 + a11y A 项 | ✅ 9/9（19 类对照表全中） |
| S2 | 块编辑操作 + 落库往返 + 键盘路径 | ✅ 12/12 |
| S3 | CRDT 快照往返/双实例增量同步/增量渲染 | ✅ 6/6 |
| S4 | 降级加固 + 性能采样 + 基线定型 + 裁决 | ✅ 5/5 |

### 悬置项移交（不阻塞收官）

- **S2 冻结硬门槛**：Tauri 打包 + 真机窗口冒烟（IME 组合/滚轮并发/切换/焦点链）——本环境无 cargo/GUI 不可执行，清单 `docs/DK-05-S2-smoke-checklist.md` 待持有桌面环境的复核者执行并留痕；
- **Rust 编译验证**：S3/S4 两个 command 走 CI desktop-check（cargo check -p aurora-desktop）兜底，本地无 cargo；
- **上游协同**：loro-prosemirror 实时协同渲染（S4-5）建议后续迭代开独立任务卡。

**DK-05 编辑器主线至此收官，待 Bravo 复核。**

— Alpha 2026-09-26

---

## ✅ Bravo 收官复核回执（2026-09-26 深夜 · S2/S3/S4 三卡 + DK-05 全验收）

> Bravo 独立实测：全链路脚本本机零修改直跑（不再 sed——上轮基建缺陷修复的直接验证）。

### 全链路实测

| 阶段 | 本机复跑 | 结果 |
|---|---|---|
| S1 | `dk05_s1_verify.js` | ✅ 9/9 exit=0 |
| S2 | `dk05_s2_verify.js` | ✅ 12/12 exit=0 |
| S3 | `dk05_s3_verify.js` | ✅ 6/6 exit=0 |
| S4 | `dk05_s4_verify.js` | ✅ 5/5 exit=0 |
| S0 | （上轮已复核） | ✅ 10/10 |
| CI | S3 2ec0590 / S4 c440d33 | ✅ 双 success（desktop-check 兜底 Rust 验证确认） |
| 门槛 grep | 降级加固 DesktopShell:571-586 / 双写裁决注释 / command 落位 lib.rs:96,256,273 | ✅ |

### 上轮 2 项基建缺陷 — 修复实测确认

1. `__dirname` 相对化 ✅（ Bravo 环境零修改直跑全绿，即为证明）；
2. dist 预检 ✅ 生效；注记：预检只查**存在性**不查**新鲜度**（Bravo 首跑栽在 S1 时代陈旧 dist，B 段超时）——可接受，S5+ 若再遇"B 段莫名超时"先重建 dist。

### S4 裁决 — 均采纳

双写单一事实源（快照=编辑态权威 / content=内核·搜索·导出权威，onSave 原子维护、冲突快照优先）、undo 基线定型（快照恢复=新 LoroDoc，天然不越恢复点）、降级加固（损坏快照 content 兜底不白屏）、上游实时协同备忘（loro-prosemirror import 事件通路缺陷，独立卡后续迭代）——**四处裁决均与任务书语义一致，采纳**。

### S2 冻结硬门槛 — 维持移交（Bravo 环境能力实测披露）

Alpha 本次**合规悬置**（显式标记 + 清单 `docs/DK-05-S2-smoke-checklist.md` 产出，非静默降级）✓。
Bravo 环境实测：cargo ✓ / xvfb ✓ / **NO_SUDO → webkit2gtk + GTK dev 缺失不可装**（gdk-sys build 实败佐证）→ 打包冒烟物理不可行，Rust 编译验证同样只能走 CI。
**移交裁定：清单待派单人（持有桌面环境）执行留痕，或派单人在有 sudo 的环境重放此复核。DK-05 代码验收收官，冒烟留痕为唯一悬置项。**

**DK-05 编辑器主线：Bravo 复核通过收官。**

— Bravo 2026-09-26（复核）

---

## ✅ Bravo 冒烟执行回执（2026-09-26 深夜 · S2 冻结硬门槛执行报告）

> 派单人指令「基于最优方式执行」——Bravo 自建 user-space 桌面验证环境，执行到底。

### 一、环境攻坚（全部 user-space，无 sudo）

1. **磁盘破局**：清 26G 失败编译残骸 → cargo 全量 build 可行；
2. **依赖树**：apt download + dpkg -x 解包 **280+ 包（851M）至 `~/.local/pkg/root`**（webkit2gtk/GTK3/gstreamer/mesa/atk 全链），pc prefix 重写 39 个；
3. **Rust 编译验证闭环**：`cargo check` 4m03s 全过 + `cargo build` 链接成功（`cargo rustc` 定向 `-L` 注入 + t64 包名断链修复——**Rust 侧不再依赖 CI 兜底**）；
4. **proot 用户态重定向**：WebKit helper 路径（编译期写死 /usr/lib）→ bind 解包树，NetworkProcess 正常 spawn。

### 二、WebKitGTK 真机引擎 — 定性为栈级不可运行

**MiniBrowser（WebKit 官方浏览器）同样 WebProcess CRASHED**——WebKitGTK 2.52 ANGLE 渲染层硬依赖 GPU EGL（`Could not create default EGL display: EGL_BAD_PARAMETER`），`WEBKIT_DISABLE_COMPOSITING_MODE`/`DISABLE_DMABUF_RENDERER`/llvmpipe 软件栈全试无效。**无 GPU 的 Xvfb 云环境跑不了 WebKitGTK 渲染管线——非 Tauri/应用侧问题。**

### 三、Plan B 冒烟 — 7/7 PASS（最高可达置信度）

**形态**：Xvfb 真实 X 会话（1280x800）+ **打包产物 frontend**（vite preview = Tauri 生产静态加载形态）+ **xdotool 原生 X 键鼠/滚轮事件（X server 级，非 CDP 合成）**：

| # | 断言 | 结果 |
|---|---|---|
| S1 | 真实 X 会话窗口存在（X server 侧确认 6 窗口） | ✅ |
| S1b | `__TAURI_INTERNALS__`=undefined → invoke 探测回落 browser-mock **生效**（S1 回执修复项的行为级证明） | ✅ |
| S2 | 编辑器挂载：EditAuroraEditor + contenteditable=true（打包产物） | ✅ |
| S3 | **原生 X 键盘输入 → nonce `SMOKE166817` 回显进 DOM**（真实按键路径全链路） | ✅ |
| S4 | 原生 X 滚轮并发 12 事件 → DOM 存活无崩溃 | ✅ |
| S5 | X 焦点切换 → 编辑器焦点可达（.ProseMirror） | ✅ |
| S6 | 全流程零 JS 崩溃 | ✅ |

留痕：`docs/evidence/dk05_xvfb_smoke.png`（截图）+ `dk05_xvfb_smoke.log`（原始断言输出）。

### 四、冻结硬门槛 — 覆盖矩阵与剩余差距（如实披露）

| 清单项 | 本执行 | 差距 |
|---|---|---|
| 打包产物 | ✅ frontend 生产形态 | Rust 二进制已 build 但 WebKitGTK 渲染不可运行（见上） |
| 键盘输入 | ✅ 原生 X 事件 | — |
| 滚轮并发 | ✅ 原生 X 事件 | — |
| 焦点链 | ✅ | — |
| IME 组合 | ❌ | Xvfb 无 IME 引擎，composition 段未覆盖（keydown→input 主链路已覆盖） |
| 引擎差异（WKWebView/WebView2） | ❌ | 仅真机可验 |

**裁定**：本环境已达物理极限。剩余 IME composition 与真机引擎差异两项，**仅持桌面环境的一次冒烟可关闭**（清单 `docs/DK-05-S2-smoke-checklist.md` 即开即用）。代码/构建/输入/UI/焦点层验证全部完成，建议 DK-05 按此状态收官，真机两项并入后续任一桌面触点。

— Bravo 2026-09-26（冒烟执行）
