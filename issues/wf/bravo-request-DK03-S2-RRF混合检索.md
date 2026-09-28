# Bravo 派发任务书 — DK-03 S2：混合检索 RRF（BM25 + 向量融合）

> 派发对象：Bravo · 生成：2026-09-28 21:55 · 基线 `d536c89`（main）
> 优先级：P1（AI 引用质量前置第二棒）· 预算：4–5 人日
> 前置：S1 向量基建（5ec6f43 已验收关闭）

## 〇、架构裁决（Alpha 冻结）

1. **组合层方案，不改 SearchBackend trait**：混合检索是组合能力（tantivy BM25 路 +
   VectorIndex KNN 路 → 融合），新建 `HybridSearcher`（core，持两者 Arc 引用）——trait
   冻结面零触碰，`SearchBackend` 语义保持「单路 BM25」不变；
2. **RRF 常数 k=60**（标准值）：`score = Σ 1/(60 + rank_i)`；双路命中（Both）天然排序靠前；
3. **各路拉取深度 = limit × 2**（BM25 top-2N + 向量 top-2N → RRF → 截断 limit），
   深度常量可配置；
4. **向后兼容**：SearchOptions/SearchHit 新字段一律 `#[serde(default)]`——旧序列化数据
   与旧调用方零破坏。

## 一、S2 范围（core 新模块 `l2_engines/hybrid_search.rs`）

### 1. 接口扩展（serde default 向后兼容）

```rust
// SearchOptions 追加：
pub mode: SearchMode,            // default Bm25 —— enum SearchMode { Bm25, Hybrid }
// SearchHit 追加：
pub source: HitSource,           // default Bm25 —— enum HitSource { Bm25, Vector, Both }
```
（S3 语义召回徽章 UI 消费 `source`——不含关键词的向量命中 = Vector 徽章的解释数据源）

### 2. HybridSearcher

```rust
pub struct HybridSearcher { /* tantivy: Arc<dyn SearchBackend>, vec: Arc<VectorIndex>, ... */ }
pub async fn search_hybrid(&self, query: &str, opts: &SearchOptions) -> Result<SearchResult, Error>
```
- 双路并发（tokio::join!）→ RRF 融合 → 截断 limit；`took_ms` 记全程；
- **snippet 策略**：Bm25 路有高亮片段用之；纯向量命中 snippet = 内容前 N 字符
  （诚实标注——S3 徽章解释来源的另一半）；
- **workspace 过滤**：S1 缺口补齐——`notevec:{id}` 记录**补 workspace_id 字段**
  （serde default = None → 当前单工作区口径视为匹配全部；KNN filter 闭包按 opts 过滤；
  backfill 重建时写回）——**S1 旧记录兼容读取**，诚实记录不回填强制；
- trash 过滤双路已各自保证（tantivy NoteDeleted 清理 / 向量 is_trashed）——混合层不加重复过滤。

### 3. 测试（mock embed + 内存 tantivy——复用 S1 mock 先例）

- 混合 round trip：关键词命中（Bm25 路）+ 语义命中（Vector 路，不含关键词）→ 混合结果
  **两类都有且 source 标记正确**；
- **RRF 性质断言**：双路都命中的笔记排序高于任一单路命中；
- 单路退化：向量表空 → 结果等价纯 BM25（mode=Hybrid 优雅降级）；
- workspace 过滤（含旧记录 None 匹配全部语义）；
- **延迟基准**：万条混合 <500ms（#[ignore] 基准单测，S1 KNN 基准先例）；
- 向后兼容：旧 JSON（无 mode/source 字段）反序列化 Default 成功。

## 二、禁改与边界

- **禁改**：SearchBackend trait 签名（组合层方案）；tantivy 内部；VectorIndex 已验收行为
  （只加 workspace_id 字段与 filter 参数——**S1 函数签名变更需在报告列明**）；
- **挂起诚实化**：BGE-reranker 精排（S3 可选）；语义召回徽章 UI（S3 Alpha 面——本卡交付
  source 数据面即可）；Private 锁定态（DK-20 合并）；移动端延迟口径（<400ms 随移动卡）。

## 三、DoD

1. `cargo test -p aurora-core` 全绿 + 新增混合面测试 ≥5 例；
2. clippy 0 + fmt 逐包净；CI 五 job 绿（Alpha API 独立确认）；
3. 万条混合基准 <500ms 断言通过（[ignore] 基准测试）；
4. 半接入三查：HybridSearcher 生产可见（lib.rs 导出）；
5. 交付报告：RRF 深度参数选择依据 + workspace 兼容策略 + 挂起项。

## 四、规约

commit 前缀 `feat(DK-03):`；阻塞写 `bravo-request-DK03S2-<主题>.md`；直接推 main。
**并行提示**：DK-02 S2 目录树（任务书 c79fadd 已就绪）与本卡领地零交集（write_path vs
l2_engines）——若 Bravo 带宽允许可双线，串行也不阻塞。

— Alpha 派发 2026-09-28
