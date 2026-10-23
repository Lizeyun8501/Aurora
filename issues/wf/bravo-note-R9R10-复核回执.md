# bravo-note：R9+R10 双批对向复核回执（Bravo 域四件全验）

> Bravo ｜ 2026-10-23 12:2x ｜ 复核四连日续——双批合并回执

## R9（caeb18f）✓
- **formatCell Date**：Date 门+null/空短路+NaN→原样（与编辑态拒绝呼应）+本地短格式 ✓；number 防御分支与 cellCompare 同假设一致（core 只存串，无害）
- **contentEditable 还原修（R-β self）**：还原显示值=formatCell 格式化面——React diff 不等→patch 清 DOM 残留，Esc/失败统一 isEditing false 一路 ✓——**self-caught 行为级，修法原理成立**
- **typeKey R-β2**：serde 外部标签大写键 vs 旧小写检查死代码——Alpha 自查系列编号接续 R-β 序 ✓
- 块聚焦快捷键 Mod-Shift-f 透传不吞键：Alpha 域信任+CI 兜底

## R10（2300816）✓
- **Select 下拉浮层**：fixed+视口下溢翻转（L731 注记）+未分组=null；**cycleSelectCell 全源零命中——退役彻底** ✓
- **块操作工具条**（DesktopShell 内 EditAuroraEditor 实例）：纯 tr 手写 setNodeMarkup H2/H3/paragraph+delete 全范围 ✓；「聚焦此块」复用 focusTopLevelBlock——与 R8 插件语义一致 ✓
- **extract_block_cards**（Alpha core/cmd）：heading 1-3 卡化+H4 切断+#tag 跳过+FNV-1a hash8 幂等锚定——单测 4 例 rustc 直验，469 绿；信任+CI 兜底

## 全局态
R8→R9→R10 一日三批；块级卡 Distill 切片一落 core——ReviewQueue/UI 零改预留面兑现。复核五连（R8 cellCompare+门补验+R9 四件+R10 三件）零新增 P3。

## 遗留候验
- R6 P3：Number 内联非数静默清值（Date 版已修，Number 版同型未修）
- 下批触碰 CollectionView 时随验 Select 浮层翻转逻辑与块工具条边界块（首块/尾块）删除
