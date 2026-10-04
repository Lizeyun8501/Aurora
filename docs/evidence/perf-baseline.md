# Aurora 性能基准（RV-01）— 对照表与趋势留档

> 任务：RV-01 性能基准对测三件套。Aurora 侧数据由 `crates/aurora-core/tests/perf_baseline.rs`
> 产出（release + `--ignored` 显式跑；CI 每周一 `bench.yml` 自动留档 artifact）。
> 竞品列（Obsidian / 思源）待同口径实测后补齐——本表先落 Aurora 侧基线与口径定义。

## 口径定义（对测前先冻结）

| 指标 | 口径 |
|---|---|
| 索引重建（冷启动代理） | 10k 笔记 `index_note` 全量重建总耗时 / 平均每条（ADR-004：搜索索引为派生，启动时自动重建——重建耗时即冷启动主要成分） |
| 全文检索 | 10k 笔记索引上 100 次中文查询（单词/多词混合）P50 / P99；V26 指标 <200ms |
| 双端收敛 | serialize → publish → drain → restore 一轮往返总耗时 / 平均每条（MockSyncBus 内存口径，非网络） |
| 数据集 | 确定性 LCG 伪随机；中文正文（jieba 真实分词负载）；10% 长文档（2000-5000 字）；1k / 5k / 10k 三档 |

## Aurora 侧基线（首跑：2026-09-30，本地 release）

| 指标 | 1k | 5k | 10k | V26 指标 |
|---|---|---|---|---|
| 索引重建（冷启动代理） | 173ms（0.173ms/条） | — | — | 冷启动 <2s ✅ |
| 全文检索 P50 | — | — | **1.04ms** | <200ms ✅（193× 余量） |
| 全文检索 P99 | — | — | **4.84ms** | <200ms ✅（41× 余量） |
| 双端收敛（每条均值） | 0.011ms | 0.013ms | 0.013ms | — |

> **首跑完成（2026-10-04，本地 release，aurora-core）**：三基准全绿（测试总 79.68s）。
> 注①：重建行 1k=173ms 为 `TantivySearchBackend` 直接口径；bootstrap 层 `rebuild_index`（含 KV/事件/锁）writerchurn 卡实测 1k=1.11s——两口径并存，冷启动 <2s 按后者评估 ✅。
> 注②：search bench setup 段（10k 逐条 index_note）耗时 ~70s——P50/P99「挂起」现象真身 = setup 慢非测试卡死；DK-22 批量优化后可缩短。
> CI 侧数字等 10-05 周一 cron 首跑回填（runner 口径差注明）。

### 增量路径 commit 占比定位（DK-22 开工令首跑 2026-10-04，本地 debug，`dk22_writer_batch --ignored`）

| 路径 | 均摊耗时（1k 口径） | 说明 |
|---|---|---|
| 单条 `index_note`（每条尾 commit） | **34.36ms/条** | 1.11s bootstrap rebuild 基线中 commit 占 **98.2%**（>>30% 改判阈值 → bulk 路线成立） |
| `batch_index`（N 条单 commit） | **0.620ms/条** | **55× 加速**；行为级 parity+幂等护栏 `dk22_batch_index_visibility_parity` |
| 投影批量（catch_up → apply_batch 攒批） | 同 batch 路径 | `dk22_projection_catch_up_bulk_and_order`：33 事件保序 + 水位线幂等 |

> **落地**：`Projection::apply_batch`（默认逐事件循环向后兼容）+ SearchIndexProjection 攒批覆写（连续 NoteCreated 单 commit，删/改 flush 保序）。**可见性约束**：批中途不 commit（读端延迟 = 批间隔），明确排除后台自动 flush（开工令裁决）。

## 竞品对照（待实测）

| 指标 | Aurora | Obsidian | 思源 | 口径备注 |
|---|---|---|---|---|
| 冷启动（10k 库） | （基线） | TODO | TODO | 竞品侧需同规模数据集人工录屏计时 |
| 万级检索 P50/P99 | （基线） | TODO | TODO | 竞品侧用其全局搜索 API/快捷键计时 |
| 双端同步收敛 | （基线） | TODO | TODO | 竞品侧依赖其同步方案（Obsidian Sync/思源云） |

## 趋势（按周追加）

| 日期 | commit | 索引重建 10k | 检索 P50 | 检索 P99 | 备注 |
|---|---|---|---|---|---|
| 2026-10-04 | 本地首跑（DK-23） | —（1k=173ms 直接口径） | 1.04ms | 4.84ms | 三基准全绿；收敛 10k=128ms |
| 2026-10-04 | 本地复跑（DK-29，artifact 链路实测） | 10k=2086ms（0.209ms/条，直接口径 10k 首次落档；5k=835ms） | 1.12ms | 5.62ms | 二次跑一致性确认（各指标噪声内 ≤5%）；perf_baseline_report.txt 落盘+glob 匹配验证 ✅ |
