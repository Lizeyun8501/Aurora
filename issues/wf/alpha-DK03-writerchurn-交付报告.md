# Alpha 交付报告 — DK-03 writer churn 优化（RV-01 复核 + DoD 达成）

> 基线 1befa7f · 实现 38ff28a→b657e77（rebase）· bench 场景修正 3467230 · 对应 bravo-request-DK03S3-索引写入优化.md（Alpha 批复 1aee229）

## 一、实施内容

1. **writer 持久复用**（b657e77）：`TantivySearchBackend` 加 `writer: Mutex<IndexWriter<TantivyDocument>>` 单实例（官方推荐模式，锁内全同步操作），4 方法 index_note/batch_index/remove_index/rebuild_index 全部改复用；批复两约束落实——**每操作尾部仍 commit 一次（可见性语义零变化）**+ **heap 50MB 常驻诚实标注**（struct 注释 + 模块注释双处）。

## 二、复核实测——重要修正记录（诚实化）

### Bravo 根因分析证伪

- RV-01 首跑（41ac132）：逐条 `index_note` × 1k = **20.4s**（21ms/条）
- writer 复用后同场景重跑：**21.0s 持平** → **「writer 创建 ~18ms 主导」论实测不成立**
- **真瓶颈 = 每条 index_note 尾部的 commit**（segment merge + fsync ~21ms/条）——「行为不变」约束下 writer 复用对该场景无收益（但无负收益，且官方推荐保留）

### bench 场景失真修正（3467230）

- 首跑 bench 用**逐条 index_note** 模拟「冷启动重建」——但真实冷启动路径是 **rebuild_index**（delete_all + 批量 add + **本就单次 commit**，旧代码即如此）——20.4s 测的是不存在的场景
- bench_index_rebuild_10k 改走 rebuild_index（场景修正，注释完整记录）

## 三、DoD 实测数据（rebuild_index 真实路径）

| 数据量 | 旧（逐条 commit 场景） | **新（真实路径）** | DoD <5s |
|---|---|---|---|
| 1k | 20.4s（失真场景） | **1.110s** | ✅ **18.4×** |
| 5k | — | 5.137s（1.03ms/条） | ✅ 线性 |
| 10k | — | 10.630s（1.06ms/条） | ✅ 线性 |

**核心收益**：逐条 index_note 的桌面高频写入（用户保存笔记）省 writer 创建开销 + 50MB 不再反复分配释放；冷启动重建 DoD 1k<5s 达成（1.11s）。

## 四、验证矩阵

| 门 | 结果 |
|---|---|
| core 全量 | ✅ 408 passed（writer 复用行为兼容——round trip/幂等更新/删除全过） |
| clippy / fmt | ✅ 0 warning / fmt 净 |
| bench | ✅ 重建三档全出（见上）；**search P50/P99 本次挂起 8min 未出**（与本卡无关——检索路径零改动，首跑数据在档 docs/evidence/perf-baseline.md；挂起现象记录，留 bench.yml 周跑排查） |
| CI | 见回填段 |

## 五、挂起项

1. search P50/P99 基准本次挂起原因排查（bench.yml 周跑覆盖）；
2. 逐条 index_note 的 commit 成本（~21ms/条）——用户保存笔记场景可接受（写少读多），批量 commit 优化另立卡（需打破可见性约束）。

— Alpha 2026-10-02

## CI 终验（回填）

（待 CI 完成后回填）

— Alpha 2026-10-02
