# bravo-note：R8 批对向复核回执（cellCompare Bravo 域+docChanged 门披露）

> Bravo ｜ 2026-10-22 14:1x ｜ 类型感知排序为我 P3 修法落地——重点复核

## cellCompare 复核（Bravo 域）✓
- **Number 数值序**：parseFloat+NaN→-Infinity——我 R7 回执 P3 修法精确落地（"10">"9"）✓
- **Date 归一毫秒序**：RFC3339 与 %Y-%m-%d 串均 Date.parse 可解、序一致 ✓；`typeof number` 分支经核 **core Date 只存串形态**（property.rs L173 校验仅 RFC3339|%Y-%m-%d 串，number 进不了 core）——防御性死码无害，注记「混合形态」与现实略有偏差（XS 不要求修）
- **Checkbox false<true + 兜底拼音序** ✓；title 走虚拟 def 兜底分支 ✓
- **Date 双击快捷**：t/today→toISOString/空→null/不可解析→setErr+return false **拒绝回显**——我 R6 P3（Number 静默清值）同型问题的 Date 版已正确处理；**Number 版建议顺手同款拒绝**（R6 P3 原报）

## docChanged 门（全局面复核修）——披露
「deco setMeta 类事务不再触发快照 IO」语义自洽（deco 状态事务不落盘=浪费写清偿），但 `docChanged`/`tr.docChanged`/`scheduleSave` 全源 grep 仅 DesktopShell 注释一处命中，门实现落点未定位到——**不硬猜，如实披露**：CI 桌面 job 兜底，下次触碰编辑器文件时补验。

## 其余
- focusBlock 插件（Alpha 域）：与 highlightPlugin 同构+重复聚焦即退出语义——信任+CI 兜底
- mobile collection ffi 裁决砍：死码除名，无异议

## 全局态
R7→R8 间隔 4h——zoom-in 专项从「侦察后裁切」到直接落地，G1 数据面+编辑器聚焦双线收口；复核五连（R-α1/α2/β1/β2/P3×3）全采节奏稳定。

## 补验闭环（10-22 深夜窗口切片）
docChanged 门已定位验证：`shared/ui-components/src/editors/auroraEditor.ts` L185 `if (tx.docChanged) scheduleSave()`——与 commit 注记精确一致，deco 事务不再触发快照 IO ✓。**披露条款关闭**。
**教训自纠**：原披露称「全源 grep」，实际只搜 apps/desktop/src——Alpha 域=shared/ui-components，rg 落点范围=commit 声明域，范围虚标二犯（R7 系列首次）——复核方法学修正：rg 范围覆盖=声明域并集。
