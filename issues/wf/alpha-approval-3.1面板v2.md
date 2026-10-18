# alpha-approval：DK-3.1 面板 v2（拖拽+持久化）——侦察批复+派单

> 批复方: Alpha ｜ 2026-10-18 ｜ 对象: d3e110a 侦察卡（窗口切片）
> **批复状态：通过——两方向一并做，按本卡直落**

## 批复要点

1. **拖拽方向同意**：aside 左边缘热区（建议 4px）+ mousedown/move/up 拖动+ width 钳制（**min 220 / max 480**，防挤垮编辑主区）——拖动中 pointer-events 注意（编辑器 iframe/编辑态干扰——拖动时 document 级监听+userSelect:none）。
2. **持久化升级同意**：开关+宽度合并 localStorage（`aurora.sidePanel` / `aurora.sidePanelWidth`）——注意现开关在 sessionStorage（d65fef5 我落的），升级时**兼容迁移：localStorage 优先，sessionStorage 兜底，无值=默认开**；browser-mock 无 localStorage 环境守卫 try/catch 沿用。
3. **InfoPanelTabs 零耦合确认**——宽度面只在 aside 容器层，不动三 tab 组件。
4. **共享文件风险预写采纳**：DesktopShell.tsx 为 Alpha 现役共享面——**rebase 三件套**（pull --rebase 前置/单职责小块提交/冲突即停回侦察）执行纪律写明。

## 派单（Bravo 执行）

**DK-3.1 面板 v2**（S ~60 行）——拖拽+持久化一并交付；完成后注记 issues/wf/bravo-3.1-面板v2.md（含钳制边界证据+localStorage 迁移说明）。
