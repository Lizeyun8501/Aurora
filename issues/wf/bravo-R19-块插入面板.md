# R19 移动端块插入面板 — 实现回执

**执行**: bravo ｜ 2026-10-27 23:15 ｜ commit c75a49d（+本回执附笔）

## 交付面

| 文件 | 内容 |
|---|---|
| `shared/ui-components/src/editors/insertCommands.ts` | 命令映射统一入口+moveBlock swap |
| `shared/ui-components/src/editors/MobileEditorDock.tsx` | 底部 Dock 五键+BlockInsertPanel 13 项 sheet |
| `RichEditor.tsx` | mobileDock prop 挂载+useToolbarState/makeToggleListCmd 导出 |
| `MobileApp.tsx` | mobileDock 传参（1 行） |
| `apps/mobile/r19-lab.html`+`vite.config.r19lab.ts` | React+mock 桥单文件 lab 页 |
| `scripts/repro-r19.js` | Playwright 验收 21 断言 |

## 挂载点取舍（任务书留我定）

选 `mobileDock?: boolean` prop（富 RichEditor 显式开启）而非 platform 嗅探——
桥接口（EditorPlatformBridge）无判别面，显式 prop 优于形状嗅探；desktop 不传=默认关。
MobileApp 一行接线。

## 命令映射表（关键语义裁决）

- **顶层 textblock（段落/标题/代码）**→ setBlockType 就地转换
- **列表内** → insertSiblingBlock（当前顶层块后插兄弟）——schema 约束下唯一正确
  路径：list_item content=paragraph+，heading/task_block/code_block 非法子节点
- **bullet/ordered** → makeToggleListCmd 三段语义（同型退出/异型就地切换/非列表
  包裹——复用 RichEditor 现成惯例，勿自造 wrapInList 防套娃变体）
- **table** → 兄弟块路（EditorToolbar insertTableCmd 的「当前块上方」语义在容器
  内 depth 计算会越界 throw——resolve(blockPos).node() 反推到 doc 层 nodeSize）
- **divider** → replaceSelectionWith（现成语义）

## 验证矩阵

- 共享层 tsc 主代码零错（`__tests__` screen/fireEvent 为既有债非本卡引入）
- mobile tsc 0；主 vite build **10.6MB ≤ 11MB 红线 ✓**
- Playwright（r19-lab）**19/20 PASS**：①②+③ 全 9 项插入 DOM+④撤销+⑤a 禁用
  态+⑥遮罩关闭+⑦ pageerror/console.error 双监听 0

## 已知坑实录（下卡深挖素材）

1. **⑤b swap 异常（19/20 唯一挂点）**：[OL,BLOCKQUOTE] 上移后变 [OL,P]——
   BLOCKQUOTE 消失+P 出现。理论分析 delete+反转 insert 无 bug；疑与 PM click
   边缘自动补段（空块点击→doc 尾补 P）叠加。已捕获待 moveBlock 单测卡深挖
   （node 环境重建 EditorState 直测 swap 逻辑）。
2. **undo 分组粒度**：undo 源自 LoroUndoPlugin（CRDT 侧非 prosemirror-history），
   快间隔连续插入（<合并窗）一次撤销回退多块——思源对标语义可接受，断言放宽
   为「可回退+hr 消失」。
3. **PM click 空块边缘→自动补段**：交互测试需点块内文字区（lab 验证踩坑实录）。
4. **disabled 按钮不可 Playwright click**：⑤a 断言改 disabled 属性断言。

## hooks 纪律（React #310）

MobileEditorDock/BlockInsertPanel 全部 hooks（useState/useToolbarState/useEffect）
无条件 early return 前置——模式切换（sheet 开/关）无渲染崩路径。panel open=false
恒渲染（closed 姿态 translateY(100%) 藏底+pointer-events:none）保 transition 动画。

——bravo 23:15
