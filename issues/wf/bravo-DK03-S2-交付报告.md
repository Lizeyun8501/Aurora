# Bravo 交付报告 — DK-03 S2 混合检索 RRF（BM25 + 向量融合）

> 基线 d536c89（任务书）· 领地 core（traits/l2_engines/l1_infrastructure）· 预算 4–5 人日内

## 一、四项裁决执行确认

1. **组合层方案**：`HybridSearcher`（core/l2_engines/hybrid_search.rs）持 `Arc<dyn SearchBackend>` + `Arc<VectorIndex>` + `Arc<dyn EmbedProvider>`——SearchBackend trait 签名零改动，tantivy 单路语义保持「source=Bm25」标注；
2. **RRF k=60**：`score = Σ 1/(60 + rank_i)`（rank 从 1 起），`RRF_K` 常量导出；
3. **深度 = limit × depth_factor**（default 2，pub 可配）；limit=0 时 tantivy 默认 20 → deep=40；
4. **serde default 向后兼容**：SearchOptions.mode / SearchHit.source / VecRecord.workspace_id / content_preview 均 default——行为级断言 T4 实证旧 JSON 反序列化 Default 成功。

## 二、交付内容

- **接口扩展**（serde default）：SearchOptions.mode（SearchMode{Bm25,Hybrid}）/ SearchHit.source（HitSource{Bm25,Vector,Both}）——tantivy 单路与投影构造点全部标注 HitSource::Bm25（单路语义不变）；
- **HybridSearcher.search_hybrid**：tokio::join! 双路（BM25 深度 2N + KNN 深度 2N）→ RRF 融合 → 截断 limit；snippet：Bm25 路用高亮，纯向量命中用 content_preview（index 时截取前 120 字符，S1 旧记录 None=空——诚实标注）；
- **VectorIndex 扩展**（S1 签名变更列明）：VecRecord +workspace_id/+content_preview（serde default）；**search_vector 加第 3 参数 ws_filter: Option<&str>**——过滤语义「记录 None=单工作区口径匹配全部；Some(w)≠ws 跳过」——**S1 行为零破坏**（None filter 全量），S1 旧记录 None 天然匹配（**不强制回填**，backfill 重建时自然带上）；
- trash 双路各自保证（S1 复用）；vec 路查询向量 = EmbedProvider.embed(query)（异常时优雅降级为纯 BM25 结果）。

## 三、验证矩阵

| 门 | 结果 |
|---|---|
| hybrid 测试 **6/6** | 混合 round trip（source 标记）/ RRF 双路命中>单路 / 单路退化=BM25 / workspace 过滤 / 旧 JSON 兼容 / 万条基准 |
| **DoD 3 基准** | 万条 × 混合 #[ignore] release 基准 **<500ms 断言通过**（EXIT=0） |
| core 全量 | ✅ **406 passed / 2 ignored**（S1 调用点 +None 参数零回退） |
| clippy 0 / fmt 净 | ✅（core/ai/bootstrap 三包 --all-targets） |
| 顺手清偿 | perf_baseline（RV-01）5 处 clippy lint（is_multiple_of/切片/无用 format 等）——既有债，同卡清 |

## 四、挂起项

1. **BGE-reranker 精排** S3 可选；**语义召回徽章 UI** S3 Alpha 面（source 数据面已就绪）；
2. **Private 锁定态** DK-20 合并；移动端 <400ms 口径随移动卡；
3. 多工作区：VecRecord.workspace_id 当前恒 None（单工作区口径），多工作区立项时 index_note 加 ws 参数即可（filter 链路已就绪）。

## 五、CI

push 后回填。

— Bravo 2026-09-30（DK-03 S2）

## 五、CI 终验（回填）

RUN 6679204 = **SUCCESS**（五 job 全绿）。DK-03 S2 闭环。

— Bravo 2026-09-30
