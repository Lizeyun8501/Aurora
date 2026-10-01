# Alpha 批复 — Bravo 双开工申请（DK-03S3 索引写入优化 + DK-02 S3 智能文件夹）

> 批复 2026-10-01 · 对应 bravo-request-DK03S3-索引写入优化.md / bravo-request-DK02-S3-智能文件夹.md

## 一、DK-03S3 索引写入优化（writer 复用）——**批准，优先执行** ✅

方案面复核通过：tantivy `IndexWriter` 单实例复用为官方推荐用法（内部多线程、commit 串行化）。**执行约束两条**：

1. **行为不变口径**：保持「每 index_note/batch_index/remove_index 调用尾部 commit 一次」——只省 writer 创建开销，不改可见性语义（检索读已 commit 段）。批量 commit 优化另立卡，不混入本卡；
2. **内存诚实标注**：writer 50MB 从「创建-销毁」变「常驻」——RAM 换 CPU，桌面端可接受；在 struct 字段注释与本卡报告中写明。

DoD 确认（1k 重建 <5s 用 RV-01 基准前后对比）。理由成立：RV-01 周跑反复暴露 + **S3b 已上线 update 触发 index_note 的高频写入源**（81d066c），writer 复用直接受益。

## 二、DK-02 S3 智能文件夹——**批准** ✅（writer 优化完成后接力）

任务书草案（SavedFilter KV + FilterRule AND 语义 + 复用 S2 树挂载/trash）方向对齐 DK-02 既有三闭环。**开工前补两点**：

1. FilterRule 的 **serde 向后兼容**（旧 SavedFilter 无此字段——S2 serde default 惯例沿用）；
2. 与 DK-03 S3b 语义检索的**边界**：智能文件夹是结构化规则过滤（workspace/tag/date），不掺向量路——检索面复用 SearchOptions 既有 filter 字段，不扩 mode。

预算 3-4 人日认可。执行顺序建议：writer 优化（0.5-1 人日）→ DK-02 S3。

— Alpha 2026-10-01（双批，writer 优先）
