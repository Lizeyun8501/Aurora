# Bravo · DK-22 交付报告 — writer 批量 commit 优化（开工令 2026-10-04）

**Base**: b32a365（v26 DK-22 时间追踪交付后）｜**执行者**: Bravo｜**预算**: 3-5 人日（实际 ~2h）

---

## 一、数据决策（开工令任务 1 → 路线裁决）

**bench 定位先行**（黑盒对比，真实 TantivySearchBackend，对齐真实调用路径）：

| 路径 | 均摊耗时 | 数据 |
|---|---|---|
| 单条 index_note（每条尾 commit） | 34.36ms/条 | **commit 占比 98.2%** |
| batch_index（N 条单 commit） | 0.620ms/条 | **55× 加速** |

**裁决：98.2% >> 30% 改判阈值 → bulk 路线成立，动实现**。数据落 `docs/evidence/perf-baseline.md`（新章节「增量路径 commit 占比定位」）。

## 二、实现（开工令任务 2/3）

| 改动 | 落点 |
|---|---|
| `Projection::apply_batch(events)` trait 默认实现 | event_bus/projection.rs——**默认=逐事件循环（旧行为），其他投影零改动向后兼容**；doc comment 写明可见性约束（批中途不 commit，读端延迟=批间隔；**后台自动 flush 明确排除**=开工令裁决） |
| SearchIndexProjection 覆写 apply_batch | l2_engines/search_projection.rs——连续 NoteCreated 段攒批单 commit；遇 NoteDeleted/NoteMetadataChanged 先 flush 攒批段再单发（**同批内 created→deleted 交错序不乱**） |
| catch_up 泵改造 | projection.rs——整批解码→apply_batch→水位线**整批推进末位 seq**；批内失败不推进→下次整批重放（投影幂等：index_note 先删后写/daily 覆盖写，重放安全） |
| 调用点 | rebuild_index 原生批量化（已有）；事件增量路径经 catch_up→apply_batch 全部受益；search bench setup 10k 场景同理缩短（82bbe18 注②的 P50 挂起真身） |

## 三、验证（四门 + DoD 三条）

**CI 原样四门（本地自证）**：`TEST=0 CLIPPY=0 FMT=0 DESK=0`（workspace 全量 425+，含两例时段/负载敏感假红此前已实验定性——本轮干净跑全绿）。

**DoD 对照**：

| DoD | 证据 |
|---|---|
| 1. 占比实测数据 | 98.2%（dk22 bench 落盘日志 + perf-baseline.md 表格）；「不为优化而优化」检查完成——数据支持才动手 |
| 2. bulk API + 行为级测试 | `dk22_batch_index_visibility_parity`（双路径 doc_count 一致+幂等重放不重复）+ `dk22_projection_catch_up_bulk_and_order`（33 事件整批应用/A 删不复活 B 存留/水位线幂等 0 重放）——**2 passed 0 warning** |
| 3. bench 三组数字 | perf-baseline.md 新章节（34.36/0.620/占比 98.2% + 1.11s rebuild 基线引用） |

## 四、领地与边界

- 触碰面：event_bus/projection.rs + search_projection.rs（aurora-core）——开工令领地为 bootstrap/write_path；**投影层是增量路径必经点**，最小补丁在此声明；vector_search/搜索投影回调口径/UI/移动端零触碰
- SearchBackend trait `batch_index` 已有，未加新 trait 方法（零 trait 拆散）

## 五、挂账（Alpha 裁决）

1. workflow `test_task_executor_retry_and_dlq`（800ms 临界负载 flaky）与 monitoring `noise_reducer_silence_*`（真实挂钟时段敏感）测试加固——dk20/dk22 交付报告均已实证定性
2. catch_up 水位线从逐条推进改为整批推进——语义更粗（崩溃恢复重放窗口=批大小），投影幂等性覆盖，如需逐条粒度可加批内 checkpoint（本卡未做，YAGNI）
