# bravo-note：R18 统计面板挂载侦察（窗口期切片——Alpha 批复零等待）

> Bravo ｜ 2026-10-25 深夜 ｜ ReviewStatsPanel 挂载点侦察+接线方案预写死（bravo 域外纯侦察，桌面接线 Alpha 拍板）

## 实锤侦察（rg 逐点）

**A 案（推荐）：InfoPanelTabs 第 6 tab**
- L2283 `useState<'backlinks' | 'graph' | 'todos' | 'history' | 'outline'>` → 加 `'stats'`
- L2285 key union 同步加 `'stats'`
- L2310 后加 `{tabBtn('stats', '复习统计')}`
- L2318 渲染链尾加 `) : tab === 'stats' ? (<ReviewStatsPanel invoke={invoke} />`
- **props 零改**：InfoPanelTabs 已持有 invoke（BacklinksPanel/GraphPanel 同款传递）；ReviewStatsPanel 交付面就是 `{ invoke: InvokeFn }` 单 prop
- 惰性条件渲染同 outline——mount 时序安全同款（EditAuroraEditor 无关，cmd_review_stats 独立拉）

**B 案（不推荐）：ReviewPanel 浮层内嵌**
- L4286 `<ReviewPanel invoke={invoke} onClose=...>` 浮层形态——评分语境贴但**关闭即丢**，统计是低频查阅面，语义不合

## 接线边界预写死
- Alpha 只动 DesktopShell.tsx 四点（两 union+tabBtn+渲染链一行）——ReviewStatsPanel.tsx/ReviewStats 面零改
- 若 Alpha 挂载时发现面板高度/条形与 InfoPanelTabs 容器不协调：ReviewStatsPanel 的 marginTop/maxHeight 可调，属 bravo 域——派单回传即可零等待修

## R-β2 三连披露（仍实锤未修）
DesktopShell L776：`setDue((r as { count: number }).count)` vs cmd_review_due_count 返回裸 `usize`——`r as {count}` 后 `.count` = **undefined**，徽标 NaN/空。R17-α2 只加了 review-dirty 重拉（load 函数体未改根）。修法二选一：core 侧改返 `{count}`（R15 面动）或 TS 侧 `setDue(r as number)`（一行）。行为级，tsc 不可见——R-β1/R-β3 同族第三例。
