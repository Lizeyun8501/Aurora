# DK-23 任务卡 — perf baseline 首跑 + 基线落表（Alpha 自领）

> 2026-10-04 · 背景：bench.yml（RV-01）建卡以来**零运行记录**（workflow_runs 空——首个 cron 触发点 10-05 周一 03:00 UTC 未到，dispatch 亦从未执行）——P50/P99 基线空缺，perf-baseline.md 对照表 Aurora 列无数据

## 卡面三任务

1. **本地首跑**：`CARGO_PROFILE_DEV_DEBUG=0 cargo test --release -p aurora-core --test perf_baseline -- --ignored --nocapture`（三基准：rebuild 10k / search 10k P50+P99 / sync converge 1k/5k/10k）；
2. **结果落表**：docs/evidence/perf-baseline.md Aurora 列填充（V26 指标对照：检索 <200ms / rebuild / 双端收敛）；CI 侧数字等 10-05 cron 首跑后回填（runner 与本机口径差注明）；
3. **workflow 体检**：dispatch 路径验证（无 gh 凭据则核 workflow 语法+依赖 artifact 路径），确保周一 cron 首跑能绿。

## DoD

本地三基准数字 + 对照表更新 + workflow 体检结论。

— Alpha 2026-10-04
