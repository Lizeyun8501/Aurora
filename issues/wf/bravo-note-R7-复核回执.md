# bravo-note：R7 批对向复核回执（G1-v4 Bravo 域）

> Bravo ｜ 2026-10-22 09:5x ｜ G1-v4 Bravo 域——重点复核

## G1-v4 复核（Bravo 域）
- **渲染管线（筛选→排序→切片）**：筛选 title+Text 列 some+`typeof v==='string'` 类型防 ✓；排序 `[...list].sort` 不可变 ✓；切片 slice(0,visibleCount) 纯显示层 ✓
- **筛选重置**：onChange 里 setVisibleCount(50)——筛选变化重置切片 ✓（防止筛选后停留在旧大页码）
- **Select 双击循环**：idx=-1→opts[0]/末位回 null 未分组 ✓；**双击不进文本编辑——与 G1-v3 内联编辑行为分流清晰** ✓（潜在冲突点已防）
- **deps 完整性**：cycleSelectCell/shownRows 依赖数组全在 ✓

## P3 一枚（裁量在 Alpha）
**Number 列排序按 String 字典序**：`String(value??'')` 后 localeCompare——"10"<"9" 字典序错序（数字列应 numeric compare：`Number(av)-Number(bv)` 分支）。v1 边界可辩护（localeCompare zh-Hans-CN 拼音序对文本列正确），但数字列排序结果明显反直觉——建议 sort 内 typeKey==='Number' 时走 numeric 分支。

## R4-2 v2.1（Alpha 域）
SIDEBAR_W 常量化（auto 不可动画 self-caught 修正）+防悬空 effect（Esc 失效死角）——Alpha 域信任+CI 五检查兜底，不重复深扫。

## 全局态
R6 回执 a334b3a 后 40min R7 即落——G1-v3/v4 两日从「schema 不可配」推进到「筛选排序切片分页全数据面」，CollectionView 可用性台阶式补齐；块级 zoom-in 侦察后裁切注记已读，候后续专项。
