# Bravo · DK-24 交付报告 — 全量测试 flaky 加固

**Base**: 2a70c96（DK-22 验收回执后）｜**执行者**: Bravo｜**预算**: 0.5-1 人日（实际 ~3h，大头为 3 轮全量 90 分钟）

---

## 一、改动清单（两测试 + 产品代码零触碰）

| # | 测试 | 根因 | 修法 | 结果 |
|---|---|---|---|---|
| 1 | `workflow::tests::test_task_executor_retry_and_dlq` | 重试链 200+400+30≈630ms 对 800ms 等待余量仅 170ms——全量并行抢 CPU 挤爆（负载敏感） | **开工令优先路线：tokio 假时钟**——`#[tokio::test(start_paused = true)]`，worker 内 backoff 与主等待同在 paused 时间轴自动快进，等待时长保持 800ms 不变（确定性瞬时完成，孤测 0.00s） | ✅ |
| 2 | `monitoring::tests::noise_reducer_silence_period` + `silence_specific_rule` | **根因实锤修正**：非「跨时间边界泛化」而是精确边界——`is_in_silence_period` 用严格小于 `now_str < end_hhmm`（"23:59"），UTC 23:59:00-59 整分钟内执行时 `"23:59" < "23:59"` 判 false → 放行 → 假红（两例失败均落在该分钟窗口） | **注入固定时刻**（2026-01-15 12:00 UTC，稳定落在 [00:00, 23:59) 区间）替代 `Utc::now()`——`should_alert` 本就注入式，产品零修改 | ✅ |

**产品代码零触碰声明**（DoD 2）：改动仅限两测试函数体 + `mod tests` 内新增 `use chrono::TimeZone`（固定时刻构造所需，非产品依赖注入点）——**无任何产品签名/行为变更**，无最小化注入点争议。

## 二、DoD 验证

| DoD | 证据 |
|---|---|
| 1. 全量连续 3 轮全绿（DEV_DEBUG=0） | `ROUND1=0 ROUND2=0 ROUND3=0`（串行落盘 /tmp/dk24r1-3.log，每轮 workspace 全量 ~25 分钟） |
| 2. 产品零触碰 | 如上声明——git diff 仅 monitoring.rs tests mod + workflow.rs 测试注解 |
| 3. CI 四门 | 本地自证 `CLIPPY=0 FMT=0 DESK=0` + TEST 三轮全绿；CI run 由 push 触发为准 |

孤测复验：workflow retry **0.00s** 绿（假时钟快进）；noise_reducer **5/5 绿 0.00s**。

## 三、认知沉淀（挂账建议）

1. `is_in_silence_period` 的 `end_hhmm` 严格小于语义意味着 **"23:59" 结尾的静默期在 23:59 整分钟失效**——若产品语义意图为「含 23:59 这一分钟」，为潜在产品缺陷（一行修复：`<=`）；本卡产品零触碰纪律未动，**挂账 Alpha 裁决是否修正产品语义**
2. 测试守则沉淀：**凡断言依赖挂钟时段/耗时边界的测试必须注入时钟**（chrono `with_ymd_and_hms` / tokio `start_paused`）——建议后续卡面 DoD 引用
