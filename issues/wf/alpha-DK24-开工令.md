# DK-24 开工令 — 全量测试 flaky 加固（Bravo）

> 发令 Alpha 2026-10-04 · 领地：aurora-core workflow/monitoring 测试（Bravo 报告挂账 1+2）· 预算 0.5-1 人日

## 背景

DK-22 时间追踪卡全量 TEST 门两例假红（Bravo 实验定性成立、Alpha 采信）：
1. `workflow::tests::test_task_executor_retry_and_dlq`——重试链 200+400+30≈630ms 对 800ms 等待余量仅 170ms，全量并行抢 CPU 挤爆（负载敏感）；
2. `monitoring::tests::noise_reducer_silence_period` / `silence_specific_rule`——silence 断言依赖真实挂钟时段，跨时间边界假红（时段敏感）。

## 卡面两任务

1. **workflow retry 加固**：等待 800→2000ms 拉长，或 tokio `pause_time` 假时钟（二选一，假时钟优先——治本；拉长兜底）；
2. **noise_reducer 去真实时钟**：silence 期断言注入 now 构造器（依赖注入消除挂钟依赖）。

## DoD

1. 全量 `cargo test --workspace --exclude aurora-desktop` **连续 3 轮全绿**（含本地 CARGO_PROFILE_DEV_DEBUG=0）；
2. 两测试改动仅限测试文件/测试构造器（**产品代码零触碰**——若需注入点必须最小化并在 commit 声明）；
3. CI 四门全绿。

## 教训继承

dk22 全套（测试住本 crate / CI 原样四门 / DEV_DEBUG=0 / 查残留进程 / clippy 位置坑）。

— Alpha 2026-10-04
