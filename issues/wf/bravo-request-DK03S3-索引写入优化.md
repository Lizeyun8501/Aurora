# Bravo request — DK-03 S3：索引写入优化（writer 复用，RV-01 实证输入）

> 提出人：Bravo · 2026-10-01 · 依据：RV-01 首跑实证（41ac132）——index-rebuild 1k 笔记=20.4s（20ms/条），10k 并发超线性 30min+ 未完；根因=`TantivySearchBackend::index_note` 每 call 新建 `index.writer(50_000_000)`（tantivy writer 创建开销主导）

## 一、方案（最小面）

`TantivySearchBackend` 持久化单个 writer 实例（`OnceLock<TantivyIndexWriter>` 或 struct 字段），index_note/batch_index/remove_index 复用同一 writer：

- tantivy `IndexWriter` 内部多线程安全（commit 串行化）——单实例复用是官方推荐用法；
- 与 S2 混合检索联动：backfill/事件接线高频嵌入场景下索引写入从 O(N×writer创建) 降为 O(1)；
- **预期**：1k 重建 20.4s → <2s（writer 创建 ~18ms/次为主导）。

## 二、范围与 DoD

1. `l1_infrastructure/search.rs`：struct 加 writer 字段（构造时创建一次）；三方法改造复用；
2. 行为不变断言：既有 search.rs 单测全绿（index_note→search round trip / 幂等更新 / 删除）；
3. **复用 RV-01 基准**：bench.yml 周跑或本地 `--ignored` 跑 1k 重建，前后耗时对比入报告（DoD：1k 重建 <5s）；
4. core 不回退 + clippy 0 + fmt 净 + CI 绿。

## 三、预算与时机

0.5–1 人日；**建议优先级高于 DK-02 S3**（RV-01 每周跑都会暴露该问题，且 DK-03 语义召回徽章 UI 上线后索引写入频率上升）。

— Bravo 2026-10-01（等批复即开工）
