# A-03 交付报告：Markdown 源码模式（与块视图双向切换）

> 执行: Bravo ｜ 工单: TA 2026-11-11 19:02 ｜ 终 commit: 2d6fb40（链: e4c3c18 feat → 2d6fb40 帮修 A-09/A-10 撞车收口）

## 首节 · 开工令假设与代码实况披露

| # | 开工令假设 | 实况 | 处置 |
|---|---|---|---|
| 1 | proptest 在依赖树（Rust proptest 1.x）| **往返序列化全在 TS 面**（docToMd/mdToNodes——auroraMarkdown.ts）——Rust proptest 测不到编辑器往返 | 等价落地: vitest property-based（mulberry32 种子 PRNG ×100 随机块树,可复现 seed 报告）;proptest 惯例保留于 Rust 测试树（既有） |
| 2 | CM6 引入走体积预算 | codemirror meta+lang-markdown ~350KB gzip 本地打包 | 零 CDN/零外链（断网可用——vite bundle 面正常）;无超预算降级必要 |
| 3 | 「保留光标位置」 | 顶层块 pos 即锚点粒度（块级,非字符级）| enter 记录 selection 所在顶层块 index→回切 focusTopLevelIndex 滚动+聚焦（工单「记录当前 blockId」字面达成——PM 无字符级稳定 id）|

## 实现清单

### shared 层（MarkdownSourceEditor.tsx + editors/index.ts 出口）
- **CM6 编辑器**: basicSetup+markdown() 语言+Mod-Enter 应用;role=region/aria-label
- **useMarkdownSourceMode 状态机**: enter（docToMd 快照+锚点）/ apply（mdToNodes,throw→E_SRC_PARSE 非静默——留源码态+「放弃改动」出口,无静默回滚）/ discard（PM doc 零污染直回）/ 回切聚焦锚点块
- **源码态禁写库**: 组件零保存通道;仅 apply 成功后 dispatch+宿主 flushSave→WritePath（ADR-003 禁旁路）
- 错误码: E_SRC_PARSE（解析失败）/ E_SRC_NOVIEW（视图失联）;错误条 role=alert
- **RichEditor**: sourceModeToggle prop——源码态 CSS 隐藏 PM host/dock（保挂载,切回零重建）;aria-pressed 切换钮（desktop）/MobileEditorDock ⋯ 更多菜单（mobile,不做键盘常驻——工单字面）
- **DesktopShell**: Mod+/（ShortcutTable sourceMode 键——可经 ShortcutSettings 自定义）+CustomEvent 跨组件通道+工具区切换钮
- **MobileApp**: sourceModeToggle 挂（更多菜单入口）

### auroraMarkdown 三修（proptest ×100 抓实三真案——往返丢块型根因）
1. **h4-h6 解析补齐**: `#{1,3}`→`#{1,6}`（E24 块面有 h4-6,解析侧漏——往返丢块型）
2. **列表吞任务行**: bullet 合并循环加 `!TASK_RE.test` 退出条件（`- [x]` 语义优先——列表后跟任务块不丢型）
3. **docToMd 块间补空行**: 相邻可合并块（列表/引用/表格/$$）md 面保块界——行式解析器天然并块根因（seed=4 案实证: 双 quote 块并一块）

### 测试
- **markdownSourceRoundtrip.test.ts**: mulberry32 种子 PRNG ×100 随机块树（heading1-6/含公式段落/task/code mermaid/quote/双列表/表格/公式/hr 交错）→md→块树**结构等价断言（零 allowlist 严格面——失败报 seed+期/得对照）;DoD 单例（表格/公式/Mermaid/任务块/h4-6 不丢块型）2/2 绿
- A-02 AttachmentPreview 测试 7/7 绿（vitest 环境修复后三对齐债+size 桩清）
- shared 全量 92/94（moveBlock 两红=**基线既有**——8f1380a clean stash 对照实证,登记 ledger 非本卡面）

### 帮修 Alpha 收口（A-09/A-10 撞车——2d6fb40）
- **根因**: A-09 与 A-10 同锚插入 lib.rs 互踩——`cmd_open_note_window` 残留 **3 个 `#[tauri::command]` 堆叠**（E0252×2——`__tauri_command_name_*` 重复生成）+ `cmd_search_advanced`/`cmd_search_notes` **失去属性**（generate_handler 引用无宏包装——联动 7 错）+ SearchExecutor unused
- c4b9ad9（A-10）基线自身 desktop-check/Test 双红实证（push 未等 CI 再现）
- 修法: 拆堆叠归位（A-10 doc+单 attr 留 open_note_window;A-09 doc+attr 回 search_advanced;Tantivy doc+attr 回 search_notes）+unused import 删——六行机械修,全错清零
- 定位法: annotation 尾只剩 E0252 碎片——attr→fn 配对扫描（逐 `#[tauri::command]` 向下 12 行找 fn 名）抓到 cmd_open_note_window 标注×3

## 验证矩阵
- 本机: shared/desktop/mobile tsc 我面零错;vitest 全库跑通（92/94,两基线红分离）;cargo fmt RC=0
- CI（2d6fb40）: **desktop-check ✅ / Clippy ✅ / Rustfmt ✅ / MSRV ✅ / Wasm×3 ✅**
- Test (stable): 唯一 FAILED=dk02_s2_trash_flow_roundtrip——**全链同名同案基线红**（trash 域 Alpha 面;A-01/A-02/A-03 三卡一致分离）;A-03 面零失败

## 双端影响面
| 端 | 影响 |
|---|---|
| 桌面 | Mod+/ 快捷键+工具区切换钮+CustomEvent 通道;ShortcutTable sourceMode 键（SHORTCUT_LABELS 同步）|
| 移动 | MobileApp sourceModeToggle;Dock ⋯ 更多菜单（源码模式行 aria-pressed）|
| 共享层 | MarkdownSourceEditor.tsx（新）/RichEditor/MobileEditorDock（扩展）/auroraMarkdown（三修）/editors index 出口 |

## DoD 对照
- ✅ 表格/公式/Mermaid/任务块往返不丢块型（proptest+单例断言）
- ✅ 源码态编辑→应用→flushSave→WritePath（onCommit 通道——搜索索引/任务投影经既有保存链更新）
- ✅ 非法输入 E_SRC_PARSE 明确错误+放弃改动钮（无静默回滚——源码保留可修正）
- ✅ 双端可用（desktop 快捷键/切换钮;mobile 更多菜单）;切换钮 aria-pressed
- ⚠️ 诚实边界: 「搜索索引/任务投影同步更新」验证=通道级（flushSave 既有链）,未做端到端 PW 断言（桌面 app 无 PW 环境依赖,登记 v2 可补真机面）

## 停手边界遵守
不做源码态协同编辑 ✅ / 不做 Vim/Emacs 键位 ✅ / 不做 Markdown 语法扩展 ✅（三修均为既有块型往返修复,非新语法）
