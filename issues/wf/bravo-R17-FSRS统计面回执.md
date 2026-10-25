# bravo-R17-FSRS统计面回执

> Bravo ｜ 2026-10-25 夜 ｜ bravo-request-R17-FSRS统计面 任务书闭环

## 交付（四 T 全落）
- **T1 revlog**：ReviewLogEntry(card_id/rating/ts)+REVIEW_LOG_CAP=10_000 环形丢最旧；追加点=add_card 首评+review_card 评分成功才记（None 早返不记）；**sync_block_cards 系统重建不入日志**（非用户评分——批量重建不污染今日计数）
- **T2 聚合**：`review_stats(now) -> ReviewStats`——total_cards/due_now（与 stats() 同口径）/today_reviews（Utc 日界 00:00 起——与 due_items 同会计口径，诚实边界）/today_again_rate（今日 0 复习→None）/due_7d（[0]=今日 24h 窗**含已到期**；[n]=(now+24n, now+24(n+1)]；第 6 天尾 168h 截止）。**锁序纪律 items 先 log 后**（add_card/review_card/review_stats/snapshot 同序防死锁）
- **T3 快照**：snapshot_json+review_log（rating 数字化 as_u32 手写零 serde 侵入）；restore **replace 语义**（清空重建防 roundtrip 翻倍）+坏条目跳过+旧快照无字段默认空（version 不 bump）
- **T4 desktop**：cmd_review_stats（手写 Value 映射对齐 cmd_due_review_cards 惯例）+注册 L228+**ReviewStatsPanel.tsx 新文件**（四指标卡+due_7d 迷你条形纯 CSS、失败静默 0 化、hot 紫 #c9a9ff 对齐 ReviewEntryCard）——**DesktopShell 零改动**（挂载面留 Alpha）

## 验证矩阵
| 门 | 结果 |
|---|---|
| TEST | ✅ `cargo test -p aurora-core` **478 passed 0 failed**（474 基线+4 新测）|
| 新测清单 | review_log_append_semantics / review_log_cap_ring_drops_oldest（10_001 全量）/ review_stats_aggregates_and_windows（+24h、+48h 边界）/ restore_old_snapshot_without_log_is_compatible |
| CLIPPY | ✅ core --all-targets -D warnings=0 |
| TSC | ✅ 0（ReviewStatsPanel 类型过）|
| FMT | ✅ 我文件 clean（fsrs.rs/lib.rs/新 tsx）|
| DESK_CHECK | 本机不可行（gdk-sys 无 GTK——任务书验证门预期内）→ CI desktop-check 兜底 |
| 接口零破坏 | stats()/due_items() 原样 |

## 域内收口披露
- fsrs.rs 本批划 bravo——顺修 R12 既有 clippy 5 错（nonminimal_bool+cloned_ref_to_slice_refs×3+useless_format，最小机械改——f796077）
- HEAD 既有 fmt 债 14 处（collection_commands/action_extractor/collection/task_projection，Alpha 域）→ **81af86f 已全仓 cargo fmt 收口**（纯格式零语义，diff 人工复验；帮 Alpha 收口披露）——CI Rustfmt 门 f796077 红实锤此债、81af86f 清偿
- 推送撞车一次（Alpha R17-α2 批并行）——rebase 后三件套复验（cmd_review_stats 注册/定义 rg 确认）再推

## 边界偏差披露
任务书预期 Rating 已有 Serialize derive——实测无。取快照手写序列化惯例（rating as_u32），core 零 serde 侵入，语义等价。

## 待办
- [ ] Alpha：ReviewStatsPanel 挂载（InfoPanelTabs 第 6 tab 或 ReviewPanel 内嵌——任务书域边界）
- [ ] Alpha：R-β2 挂账未修（review_due_count 裸 usize vs {count} 解构）
- [ ] CI：Test/Clippy/desktop-check/MSRV 后台跑完（wasm×3+Rustfmt 已绿）
