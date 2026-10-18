# bravo-request：DK-3.1 右侧常驻面板 v2 扩展侦察（窗口切片，2026-10-18）

> 对象: d65fef5 3.1 v1（InfoPanelTabs 常驻化）；v1 边界自注"无宽度拖拽/无持久化布局记忆"——本报告预勘两方向接入点，批复后派单零等待。

## 面结构（DesktopShell.tsx，单文件内聚）

- **aside 段 L2920-2962**：`width: 300` 硬编码+`borderLeft` 分隔视觉+`×` 关闭钮（sessionStorage `aurora.sidePanel`）
- **开关 state L2171-2174**：`sidePanelOpen`，默认开（`getItem !== '0'`）
- **InfoPanelTabs L1573+**：反链/图谱/待办三 tab，props 面干净（invoke/noteId/onSelect）
- **EditorPane 零耦合**：aside 是兄弟节点——**v2 两方向全部壳层独立，EditorPane/InfoPanelTabs 预判零触碰**

## 方向①宽度拖拽

- 接点：aside 前插 4px 拖拽条 div（`cursor: col-resize`）+ mousedown→document mousemove/mouseup 全局监听
- 宽度 state（`sidePanelWidth`，钳制 [220, 560]）+ localStorage `aurora.sidePanelWidth` 持久化
- 关闭钮/联动逻辑不动——风险低

## 方向②布局持久化升级

- sessionStorage→localStorage（会话级→跨会话），key 不变 `aurora.sidePanel`（+宽度新 key）
- 三处 setItem/getItem 同步换；注意 SSR 无关（纯桌面 webview）

## 风险与边界

- **DesktopShell.tsx 巨石（2900+ 行）共享文件**——落卡后按 rebase 撞车三件套纪律（tsc+check+关键接线 rg 复核）再 push
- v1 面验收刚过（d65fef5 本机 tsc0+build 0）——v2 改动段与 v1 段重叠，建议 v2 派单在 v1 验收回执后

## 预估

S 卡一落：拖拽+持久化合计 ~60 行增量（拖拽条组件+state+两个 key），零新数据面零后端触碰。请 Alpha 批复方向取舍（①②可并落或只①）。
