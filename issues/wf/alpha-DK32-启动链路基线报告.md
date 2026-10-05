# DK-32 — 桌面启动链路耗时分解基线报告（Alpha 自立卡）

> 2026-10-05 · release 口径 10k 笔记 · `cargo test -p aurora-bootstrap --test dk32_startup_baseline --release --ignored --nocapture` · 1/1 绿

## 一、数据

| 指标 | 耗时 | 说明 |
|---|---|---|
| **二次启动（10k 存量）** | **153 ms** | **核心指标——用户日常体感**；存量核验 10000/10000 完整 |
| 首建启动（全新库） | 276 ms | 首装语义（含迁移执行+DEK 创建） |
| 分段·migration | 24 ms | SQLite 迁移管理器（存量库=幂等跳过，实测 24ms 为全新执行） |
| 分段·vault（DEK 保险库） | 0 ms | 已有 KEK 加载路径 |
| 差值·core+startup+其余 | ≈129 ms | 投影 rebuild（tags 21ms——DK-28）+索引 open+ai 预载+boot 备份，粗粒度免插桩口径 |

**决策线判定（<500 优 / <1s 良 / >1s 须优化）：二次启动 153ms = 优（2.3 倍余量至 500ms 线）**

## 二、口径注记（诚实边界）

1. **分段方法为探针+差值**：不改产品代码不插桩——migration/vault 公开构件在独立临时库单独计时（与 bootstrap 内部同实现，数值可直接对照）；core+startup 为差值近似；
2. **10k 注入 32.6s 为测试基建开销**（逐条 `set` 无事务聚合），非用户面写路径基准——顺带印证 DK-22 writer 批量 commit 优化在真实写路径的价值空间；
3. 首建 276ms 中 migration 为全新执行（存量二次启动时迁移幂等跳过，耗时更低）。

## 三、结论与建议

- **启动链路全绿：免优化**（153ms << 500ms 优线）——文档化基线，与 DK-25 search P50=1.04ms、DK-28 tags 21ms 同入性能资产表；
- **无需立优化卡**；若未来启动劣化（如投影面扩张），本 bench 可直接复跑对照（决策卡惯例）；
- **资产沉淀**：`dk32_startup_baseline` 加入 bench 惯例族（dk25/dk28/dk32——CI 周一 cron 已覆盖 cargo bench 路径的测试型 bench 需 --ignored 显式触发，暂维持本地跑+报告落档模式）。

— Alpha 2026-10-05
