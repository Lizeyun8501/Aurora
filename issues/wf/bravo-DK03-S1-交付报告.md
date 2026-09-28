# Bravo 交付报告 — DK-03 S1 向量基建与嵌入管线

> 基线 66d7f9e（任务书）/ f8151a0（代码基线）· 领地 aurora-core + aurora-ai + bootstrap 挂载 · 预算内

## 一、两项改判执行确认

1. **tantivy 保留主路径**：未动 tantivy 管线一行（禁改面零触碰）；
2. **纯 Rust 暴力 KNN**：`VectorIndex` 内存缓存 + 余弦遍历，零新依赖零 unsafe；sqlite-vec 未引入（>5 万条再评估）。

## 二、做了什么

| 组件 | 位置 | 语义 |
|---|---|---|
| `VecRecord` | core/l2_engines/vector_search.rs | `notevec:{note_id}` → {dim, vector, model, content_hash} |
| `EmbedProvider` trait | 同上 | 嵌入供给抽象（core 侧零 HTTP 依赖——Ollama 实现在 ai crate） |
| `VectorIndex` | 同上 | index_note（hash 去重 + dim/model 不匹配淘汰重建）/ search_vector（余弦 KNN + trash 过滤）/ backfill_missing（限速回填）/ refresh_cache |
| `OllamaEmbedProvider` | ai/embed.rs | reqwest 直连 `/api/embeddings`（真发请求 + 维度守卫——不复用 OllamaProvider，其 chat 探测/降级语义与嵌入无关） |
| `EmbedFromAiProvider` 适配器 | ai/embed.rs | `AIProvider → EmbedProvider` 转发——**DK-10 门禁自动生效**（Deny 工作区 embed PermissionDenied 零 HTTP） |
| `BootedApp.vector_index` | bootstrap | 生产挂载（nomic-embed-text/768 口径）——事件接线挂 S2 |
| trash 联动 | search/backfill | 复用 DK-02 `trash:` 前缀语义（软删笔记不参与检索，向量保留供恢复） |

## 三、验证矩阵（本地）

| 门 | 结果 |
|---|---|
| `cargo test -p aurora-core --lib vector_search` | ✅ **5/5**（round trip / trash 过滤 / 维度淘汰 / hash 去重 / backfill） |
| `cargo test -p aurora-ai --lib embed` | ✅ **3/3**（Deny 零 HTTP / Ollama round trip / 维度守卫） |
| **DoD 3 基准**：万条 × 768 维 KNN（#[ignore]，release） | ✅ **assert <50ms 通过**（`--ignored` 本地跑 EXIT=0；0.57s 含万条 seed 构建） |
| clippy -D warnings（core/ai/bootstrap --all-targets） | ✅ 0（修：MutexGuard 跨 await→scan 锁外先行；EmbedFromAiProvider 死 dim 字段移除） |
| fmt 逐包 | ✅ 净 |
| mock 策略 | CI/本地全 mock（MockEmbed 确定性 hash 向量 + mockito Ollama stub）；**真实 Ollama 联调挂起**（本机无 Ollama——真机联调随 APK 冒烟，DoD 5 诚实化） |

## 四、挂起项（诚实化）

1. **Private 工作区锁定态内存索引**（DK-03 DoD 3 加密面）——S2 与 DK-20 合并裁决（任务书既定）；
2. **reranker** S3 可选；**sqlite-vec** >5 万条再评估；
3. **事件接线**（保存/恢复事件 → 异步 embed）S2 融合——S1 已交付原语 + BootedApp 挂载 + backfill 命令；
4. 真实 Ollama 联调随 APK 冒烟。

## 五、CI

push 后回填。

— Bravo 2026-09-28（DK-03 S1）
