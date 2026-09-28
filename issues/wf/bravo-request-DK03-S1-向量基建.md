# Bravo 派发任务书 — DK-03 S1：向量基建与嵌入管线

> 派发对象：Bravo · 生成：2026-09-28 19:50 · 基线 `4226161`（main）
> 优先级：P1（AI 引用质量前置——检索质量决定 aiLiquify 上下文质量）
> 预算：6–8 人日（S1；S2 混合 RRF 4–5 人日预告在下）

## 〇、两项架构改判（Alpha 裁决，理由入档——卡面文字让位工程理性）

1. **tantivy 保留为主路径，不迁 FTS5**：卡面「FTS5 主路径」的意图是可靠全文检索+中文分词——
   `TantivySearchBackend`（含 jieba_tokenizer）已在生产管线（投影/水位线/重建）运行多版本，
   性能与功能均满足；硬迁 FTS5 = 重写整条索引管线买不到新能力。改判记录入卡。
2. **向量检索 S1 用纯 Rust 暴力 KNN，不装 sqlite-vec**：卡面「sqlite-vec 优先」的意图是有
   向量检索——但 sqlite-vec 是 loadable extension（跨平台二进制分发是 V26 反模式的坑），
   而 DoD 规模（万条笔记 × 768 维暴力 KNN ≈ 10ms 级）远够用。**>5 万条再评估 sqlite-vec**。
   纯 Rust = 零分发风险 + 零 unsafe。

## 一、S1 范围（crates/aurora-core 新模块 `l2_engines/vector_search.rs` 或就近）

1. **向量表（KV 存储）**：`notevec:{note_id}` → { dim, vector: Vec<f32>（量化可选）, model: String }；
   删除/恢复与 DK-02 trash 联动（trash 中笔记不参与检索——**复用 is_trashed 谓词**）；
2. **嵌入管线**：
   - `EmbedProvider` 抽象：Ollama 本地（`/api/embeddings`，主路径）+ 云端 fallback
     （**复用 DK-10 门禁**——OpenAiCompatProvider.embed 已 fail-closed，Deny 工作区自动走本地）；
   - 维度配置化（默认 768，nomic-embed-text 口径；模型/维度不匹配的旧向量淘汰重建）；
3. **索引管线**：笔记保存/恢复事件 → 异步 embed（去重：内容 hash 未变跳过）；批量回填命令
   （boot 后缺向量笔记补齐——限速防 Ollama 过载）；
4. **暴力 KNN 检索**：`search_vector(query_vec, k, filter) -> Vec<(note_id, score)>`——
   内存缓存向量表 + 余弦相似度；trash 过滤复用 is_trashed；
5. **单测**（Ollama 不可用的 CI 环境：mock embed provider——mockito 先例）：
   索引→检索 round trip / trash 过滤 / 维度不匹配淘汰 / 内容 hash 去重 / Deny 工作区 embed
   走本地（零 HTTP 断言复用 DK-10 模式）。

## 二、禁改与边界

- **禁改**：tantivy 管线（S2 才融合）；`SearchBackend` trait 签名（S2 扩展时统一）；投影逻辑；
- **挂起诚实化**：Private 工作区锁定态内存索引（DK-03 DoD 3——涉及加密面，S2 与 DK-20 合并裁决）；
  reranker（S3 可选项）；sqlite-vec（>5 万条再评估）。

## 三、DoD

1. `cargo test -p aurora-core` 全绿 + 新增向量面测试 ≥5 例；
2. clippy 0 + fmt 净（逐包）；CI 五 job 绿（Alpha API 独立确认）；
3. 万条向量 KNN 延迟断言 <50ms（基准测试或 #[ignore] 基准单测——DoD 500ms 的 10 倍余量）；
4. 半接入三查：EmbedProvider/KNN 生产可见（lib.rs 导出），非仅测试存在；
5. 交付报告：裁决执行确认 + mock 环境测试策略 + 真实 Ollama 联调挂起声明（本机无 Ollama
   则 CI 全 mock，真机联调随 APK 冒烟）。

## 四、规约

commit 前缀 `feat(DK-03):`；阻塞写 `bravo-request-DK03S1-<主题>.md`；直接推 main。

— Alpha 派发 2026-09-28
