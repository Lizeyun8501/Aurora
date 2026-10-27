# R19 移动端块插入面板 派工单（编辑器移动体验对标思源）

**派给**: Bravo ｜ **派单**: Alpha 2026-10-27 22:30 ｜ **来源**: 用户真机验收需求（思源移动端对标，截图见 upload/6ae0b1912c7202d451a08656_1000030452.jpg）

## 背景

用户验收 v24.1 后提出：参照思源笔记移动端编辑器，补齐块插入交互。思源该界面 = 编辑器底部工具栏（`+`/块属性/撤销重做/块上移下移）+ 点 `+` 弹两列网格块类型菜单（H1-H6/无序/有序/任务列表/引述块）。

**Aurora 现状差距**：移动端 RichEditor（共享层 lazy 挂载）**无任何块插入工具栏**——移动用户只能靠 Markdown 语法敲 `# `/`- ` 变块，触屏下这个输入成本不可接受。共享层 schema 块类型已齐全（heading 1-6/taskItem/blockquote/table/code_block/math/mermaid/divider——R 系列全落地），**本卡纯 UI/命令层，零 core/Rust 改动**。

**许可证红线**：思源 AGPLv3，严禁复制其源码/组件结构——只对标交互语义，实现走 Aurora 共享层自有体系。

## 目标

移动端编辑器补两个组件（均落 `shared/ui-components/src/editors/`，桌面将来可复用触屏形态）：

### 1. MobileEditorDock（底部常驻工具栏）

- 按键：`+`（插入面板开关）｜撤销｜重做｜块上移｜块下移
- 撤销/重做状态感知（可参考 `RichEditor.tsx` 的 `useToolbarState`——canUndo/canRedo 逻辑现成）
- 块上移/下移：PM Selection 对齐块边界 + `lift`/自定义 swap（R8 块聚焦系列有可参考的块定位工具 `focusBlock.ts`）
- **键盘适配**：dock 需在软键盘弹出时贴键盘上沿——用 `useKbdViewport` 已注入的键盘视口 CSS 变量（mobile 侧现成），共享层组件只消费变量勿自己算
- `+` 点开 BlockInsertPanel 半屏 sheet；再点/选完块/点遮罩关闭

### 2. BlockInsertPanel（两列网格块菜单）

本卡必做项（schema 全现成，直接映射）：

| 菜单项 | schema 命令 |
|---|---|
| 一~六级标题块 | heading level 1-6（setBlockType） |
| 无序列表块 | bullet_list（wrapIn） |
| 有序列表块 | ordered_list（wrapIn） |
| 任务列表块 | task_list/taskItem（wrapIn） |
| 引述块 | blockquote（wrapIn） |
| 代码块 | code_block |
| 表格块 | table（R2.7 系列四命令手术有 insert 命令可参考） |
| 分割线 | divider |

**明确不做**（停手线）：音频块/表情选择器（schema 无 audio/emoji 节点）、文档属性区（标签/图标/题头图——Aurora 无标签系统，涉及 core schema+存储，独立产品裁决后另派 DK 卡）。

## 设计要点

1. **插入命令映射表**统一封装（`insertCommands.ts`）：`(view: EditorView, type: BlockKind) => boolean`，dock/panel 都走这层——勿散落 dispatch
2. sheet 交互：底部半屏+圆角+遮罩，CSS 动效 ≤200ms（触屏延迟感阈值）
3. 图标：**SVG inline 手绘**，勿引图标库（vite singlefile 包体红线——当前 10.6MB，勿超 11MB）
4. 挂载点：`RichEditor.tsx` 内部条件渲染（新 prop `mobileDock?: boolean`，mobile 侧传 true——桌面默认关），或 mobile 侧独立挂——**你定**，取舍写进回执
5. 触屏：panel 按钮用 `onClick`（WebView click 合成良好）+ 足够热区（≥44px）

## 边界（停手线）

- **勿动**: schema/auroraEditor.ts 命令核心/focusBlock.ts 内部逻辑/aurora-core 任何 Rust 面
- 桌面 EditorToolbar 不动不并（形态不同，将来统一是另一个卡）
- 标签/题头图/emoji/音频——上文明示不做
- mobile-ffi/MainActivity 桥——本卡纯前端，零 JNI 改动

## DoD

1. **Playwright 回归**（本卡核心验收，参考 `/home/z/my-project/scripts/repro-list2.js` 的 mock 桥模式）：
   - 加载 dist/index.html + mock 桥 → 进编辑器 → 点 `+` → 逐项插入 H1/H2/无序/有序/任务/引述/代码块 → DOM 断言（h1/ul/ol/li[data-task]/blockquote/pre 出现且内容正确）
   - 撤销：插 H1 后点撤销 → DOM 断言 h1 消失
   - 块上移/下移：两块场景 swap 断言
   - **errors === 0**（pageerror+console.error 双监听）
2. `tsc --noEmit` 干净；vite build 后包体 ≤11MB
3. **hooks 纪律**（v24.1 真机教训——React #310 零容忍）：所有新组件 hooks 声明无条件 early return 之前；`npm run lint` 若有 react-hooks 规则必须零告警
4. 既有移动端回归不破（repro-list2.js 树↔列表 0 错误）
5. 注记 issues/wf/bravo-R19-块插入面板.md（挂载点取舍/命令映射表形状/坑）；push main 直推
6. 出包验证可选（CI tag apk-v25 或本机增量——纯前端 gradle 18s 暖缓存路径）

## 已知坑

- **React #310**（昨日 v24.1 真机首报根因）：条件 early return 绕 hooks 声明 → 渲染崩。本卡新组件直接面对模式切换（sheet 开/关）——hook 声明全前置
- useKbdViewport 的键盘视口变量是 mobile 侧注入的 CSS 自定义属性——共享层组件读 CSS 变量即可（勿 import mobile 模块，会破共享层方向）
- PM `wrapIn` 对已有列表项重复调用是幂等嵌套——插入前检查 `$from.parent` 类型避免套娃
- WebView 里 `position: fixed` 在键盘弹出时的 viewport 变化——依赖键盘视口变量而非 window resize
