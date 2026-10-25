# bravo-note：R17 对向复核回执（大纲实时化）

> Bravo ｜ 2026-10-25 午后 ｜ Alpha 域批——复核十三连（短）

## R-β3 消解确认 ✓
collectOutline（L2197）PM doc.descendants 单一事实源——文本行解析面整体退役，代码块伪 heading 根除；遍历序=跳转面 `n === idx` 同一语义 ✓。

## 全验 ✓
- 三派发点（创建完成 L2791/onUpdate L2808/outline-request 应答 L2834）覆盖全 ✓
- **首帧时序安全**：OutlinePanel 惰性条件渲染（L2307 `tab === 'outline' ?`）——mount 时主区监听器必已挂，request 必有应答，无空帧脆弱 ✓
- mount 同步 request+同帧 sync 零闪烁 ✓；`Array.isArray` 防御 ✓
- 同名 heading 撞车防线保持（index 语义延续）✓

## 全局态
R-β2 仍未修（行为级挂账）。本回执后 Bravo 开工 bravo-request-R17-FSRS统计面（fsrs.rs/lib.rs/ReviewStatsPanel 三面，DesktopShell 禁碰纪律已领）。
