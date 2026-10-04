# Bravo · DK-22 交付报告 — 时间追踪与番茄专注

**Base**: d4a98c0（DK-20 验收回执后）｜**执行者**: Bravo｜**日期**: 2026-10-04

---

## 一、交付内容（卡面四任务 → 落点）

| # | 任务 | 落点 | 状态 |
|---|---|---|---|
| 1 | 计时会话 API + record_actual_minutes 桥接 | **既有基建确认可用**（4feed5e DK-06 第三批：FocusSession start/end/settle_session_to_task + TaskProjection.record_actual_minutes 累加守卫点 248 行）——本卡增量补齐其后半链 | ✅ |
| 2 | 后台挂起补偿不丢秒 | `PomodoroTimer.advance_seconds(n)`（批量推进,与逐秒 tick 语义一致含跨 phase 切换）+ `PomodoroState.wall_anchor` 挂钟锚（serde default 向后兼容）+ `sync_wall_clock()`（运行态按 now-anchor 差值一次补齐后重锚;非运行态/无锚 no-op）+ tauri `cmd_pomodoro_sync` + 前端 visibilitychange 接线 | ✅ |
| 3 | 番茄钟 25/5 可配置 + 专注 UI | core `PomodoroTimer(work,break)` 已有 → 本卡接出 tauri 7 命令（state/start/pause/reset/config/sync/weekly_review, OnceLock 全局实例）+ `FocusPomodoro` 组件（phase 色彩双载体/开始暂停重置/配置输入 1-180|1-120 校验/browser-mock 提示态） | ✅ |
| 4 | 周回顾预计 vs 实际偏差率 | `TaskProjection::weekly_summary(task_ids)` → WeeklySummary{total_estimate,total_actual,deviation_rate,per_task}（除零防护: total_est==0→None; 幽灵 id 跳过; per_task 单行偏差复用 deviation_ratio 同式）+ `cmd_weekly_review` + `WeeklyReview` 面板（汇总偏差率红绿着色+每任务明细表） | ✅ |

**口径诚实化**：周回顾任务集 = 前端传「今日任务行 ids」（cmd_today_task_rows）——面板标题已如实标注「今日任务样本」。「全周完成任务集查询」需投影时间维度扩展，不在本卡范围。

## 二、四门验证矩阵

| 门 | 命令 | 结果 |
|---|---|---|
| TEST | `cargo test --workspace --exclude aurora-desktop`（落盘全量×2 轮） | **dk22 孤测 6/6 绿**;全量 22 target 中 2 个**时段/负载敏感假红**（见三） |
| CLIPPY | `cargo clippy --workspace --exclude aurora-desktop --all-targets -- -D warnings` | **0 告警 ✅** |
| FMT | `cargo fmt --all -- --check`（独立跑） | **0 ✅** |
| DESK | `PKG_CONFIG_PATH=/tmp/fakepc cargo check -p aurora-desktop` | **✅** |
| 前端 | `tsc --noEmit` + `vite build` | **TSC=0 / VITE=0 ✅** |

### dk22 测试名单（grep 核验，防 feature 门控静默吞测）

- `l2_engines::task_projection::tests::dk22_weekly_summary_deviation`（多会话累计 50+40 求和/聚合偏差率/幽灵 id/未估 None）→ ok
- `l2_engines::task_projection::tests::dk22_weekly_summary_no_estimate_is_none`（除零防护）→ ok
- `l3_domain::today_view::tests::dk22_wall_clock_compensation_keeps_seconds`（90s 补齐+重锚防重复扣秒，**DoD1**）→ ok
- `l3_domain::today_view::tests::dk22_compensation_spans_phase_change`（60s 工作跨切 30s 休息+cycle 计数）→ ok
- `l3_domain::today_view::tests::dk22_pause_freezes_and_start_reanchors`（暂停冻结/恢复重锚）→ ok
- `l3_domain::today_view::tests::dk22_serde_backward_compat_wall_anchor`（旧 JSON 反序列化 None）→ ok

## 三、全量 TEST 门的两例假红——实验定性（非本卡回归，诚实披露）

| 测试 | 定性实验 | 结论 |
|---|---|---|
| `workflow::tests::test_task_executor_retry_and_dlq` | 挂后 stash 回基线复跑**绿**;dk22 态孤测 **3 连跑全绿**（0.80s×3） | 重试链 200+400+30≈630ms 对 800ms 等待**余量 170ms**——全量并行抢 CPU 时被挤爆的**负载敏感 flaky** |
| `monitoring::tests::noise_reducer_silence_period` / `silence_specific_rule` | 全量挂（0.02s 内断言）→ 孤测 **5/5 绿（0.00s）** | silence 期断言依赖**真实挂钟时段**——全量跑跨真实时间边界时假红的**时段敏感 flaky** |

两例均非 DK-22 改动面（workflow.rs / monitoring.rs 本卡零触碰）。CI runner 单线干净环境为真实裁决。

## 四、挂账（Alpha 裁决）

1. **workflow retry/DLQ 测试加固**：等待拉长（800→2000ms）或 tokio `pause_time` 假时钟——消除 800ms 临界
2. **noise_reducer silence 测试去真实时钟**：注入 now 构造器——消除时段敏感
3. **周回顾「全周完成任务集」查询**：投影需时间维度（completed_at），现口径=今日样本
