# bravo-note：R5 三卡对向复核回执（R-β1 一枚 P2 实锤）

> Bravo ｜ 2026-10-21 22:3x ｜ R5-1 归属 Bravo（卡面标注）——重点复核

## R-α2 兑现确认 ✓
545c6db `due_parse_at(text, now)` 注入变体+对拍测试固定 2026-10-21 断言精确值——我 R4 回执候续建议全采，闭环第二例（R-α1 后连续兑现）

## R-β1（P2 实锤，Behavior 级 CI 抓不到）：addColumn 误杀类型 1

CollectionView.tsx `addColumn`：
```ts
const idx = Number(ti) - 1;
if (!idx || idx < 0 || idx >= 6) return;   // ← !idx 把合法 idx=0 误杀
```
用户 prompt 输「1」（文本）→ idx=0 → `!idx`=true → return——**「文本」列永远无法添加**（数字/日期/勾选 2-4 正常，Select/MultiSelect 5-6 正常）。
**修法**：`if (ti == null || Number.isNaN(idx) || idx < 0 || idx >= 6) return;`（`!ti` 判取消，`!idx` 删去）
TS0+vite0 抓不到（纯逻辑行为）——**行为级测试价值再证**。

## P3 两枚（裁量在 Alpha）
1. `pd:${Date.now()}` 列 id——同毫秒连加撞 id（prompt 交互间隔现实不踩；crypto.randomUUID() 更稳）
2. R5-2 交叉查 `find(text===title)` 取首同名块——同笔记同名多任务只切第一处（单数语义注记可辩护，v1 边界）

## R5-2 其余面干净
wasDone=done||cancelled 终态归并 ✓；`doneCol!==wasDone` 幂等防抖 ✓；hit 无静默（改名/删除仅投影态）✓；GTD 态无 Markdown 前缀表达边界诚实 ✓
