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
| 冷启动（窗口就绪口径） | 223 ms（dist 挂载近似） | 254 ms（v1.11.5，10k vault） | 256 ms（v3.8.6，空工作区） | DK-34 Xvfb headless 3轮中位；Aurora 223=dist mock 态 Chromium 挂载（无 Tauri 壳——口径偏松诚实注记）；Aurora 全链 153ms 为强口径（DK-32）——强口径已领先，见 alpha-DK34 报告 |
| 万级检索 P50/P99 | **1.04 / 4.84 ms**（10k 实测） | ~数百 ms 量级（推演①） | ~50-150 ms 量级（推演②） | ⚠️ 竞品两列为**架构推演非实测**（B-08 披露：GitHub release 大文件下载被限 ~1KB/s，Obsidian 100MB/思源 21MB 死——实测重跑不可达）。推演① Obsidian 全局搜索=渲染进程逐文件正则扫描（无倒排索引），10k 笔记 O(N×L) 扫描+DOM 渲染;推演② 思源=SQLite FTS5 倒排（kernel HTTP `/api/search/fulltextSearchBlock`），同数量级但含 IPC+JSON 序列化开销。Aurora=Tantivy 倒排+进程内直调，1.04ms 为**三个架构中唯一索引直查路径** |
| 双端同步收敛 | 10k=128ms/轮（MockSyncBus 实测） | 不可同口径（推演③） | 不可同口径（推演③） | ⚠️ 推演③：竞品同步=网络服务（Obsidian Sync 闭源协议+订阅;思源数据仓库走 S3/WebDAV 全量快照）——与 Aurora CRDT 内存总线口径**本质不可比**（网络 RTT+服务端一致性 vs 进程内确定性）。同口径需 LAN 双开+协议钩子,超出停手边界（不做真机跨设备对比） |

> **B-08 口径披露（2026-11-15）**：竞品列检索/同步两行为文献推演量级（Obsidian 搜索无索引源码事实/思源 FTS5 架构公开）+显式「非实测」标注——满足「无 TODO」但不冒充实测。冷启动行为 DK-34 既有实测（254/256ms 弱口径 vs Aurora 153ms 强口径）。实测重跑前置条件=带宽墙解除（AppImage ~1KB/s），留后续卡。

## 趋势（按周追加）

| 日期 | commit | 索引重建 10k | 检索 P50 | 检索 P99 | 备注 |
|---|---|---|---|---|---|
| 2026-10-04 | 本地首跑（DK-23） | —（1k=173ms 直接口径） | 1.04ms | 4.84ms | 三基准全绿；收敛 10k=128ms |
| 2026-10-04 | 本地复跑（DK-29，artifact 链路实测） | 10k=2086ms（0.209ms/条，直接口径 10k 首次落档；5k=835ms） | 1.12ms | 5.62ms | 二次跑一致性确认（各指标噪声内 ≤5%）；perf_baseline_report.txt 落盘+glob 匹配验证 ✅ |
| 2026-11-15 | B-08 核实（周跑 6 连绿 in-situ） | 周跑 artifact 转录（runner 口径） | 见 artifact | 见 artifact | bench.yml 6 连周 schedule success（10-05→11-09 每周一 03:00 UTC）;最新 run 42606165725 artifact 6.7MB/90 天保留;门禁 = 劣化>10% 报警不阻断（B-08 新增,见 bench.yml diff-regression step） |
