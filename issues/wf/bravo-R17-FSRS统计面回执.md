# bravo-R17-FSRS统计面回执

> Bravo ｜ 2026-10-25 夜 ｜ bravo-request-R17-FSRS统计面 任务书闭环

## 交付（四 T 全落）
- **T1 revlog**：ReviewLogEntry(card_id/rating/ts)+REVIEW_LOG_CAP=10_000 环形丢最旧；追加点=add_card 首评+review_card 评分成功才记（None 早返不记）；**sync_block_cards 系统重建不入日志**（非用户评分——批量重建不污染今日计数）
- **T2 聚合**：`review_stats(now) -> ReviewStats`——total_cards/due_now（与 stats() 同口径）/today_reviews（Utc 日界 00:00 起——与 due_items 同会计口径，诚实边界）/today_again_rate（今日 0 复习→None）/due_7d（[0]=今日 24h 窗**含已到期**；[n]=(now+24n, now+24(n+1)]；第 6 天尾 168h 截止）。**锁序纪律 items 先 log 后**（add_card/review_card/review_stats/snapshot 同序防死锁）
- **T3 快照**：snapshot_json+review_log（rating 数字化 as_u32 手写零 serde 侵入）；restore **replace 语义**（清空重建防 roundtrip 翻倍）+坏条目跳过+旧快照无字段默认空（version 不 bump）
- **T4 desktop**：cmd_review_stats（手写 Value 映射对齐 cmd_due_review_cards 惯例）+注册+**ReviewStatsPanel.tsx 新文件**（四指标卡+due_7d 迷你条形纯 CSS、失败静默 0 化、hot 紫 #c9a9ff 对齐 ReviewEntryCard）——**DesktopShell 零改动**（挂载面留 Alpha）

## 验证矩阵
| 门 | 结果 |
|---|---|
| TEST | ✅ `cargo test -p aurora-core` **478 passed**（474 基线+4 新增：revlog 追加语义/快照 roundtrip 含 review_log/统计聚合/7d 窗口边界）+ doctest/doc 5 绿 |
| Clippy | ✅ -D warnings 0（含顺修 R12 遗留 5 错——域内收口 f796077） |
| Rustfmt | ✅（81af86f 全仓 fmt 收口——HEAD 既有 14 处债清偿） |
| MSRV | ✅ CI 终证（1044090 落盘） |
| tsc | ✅ 组件独立类型过（挂载面类型由 Alpha 接线后复验） |

## 域内收口披露
- fsrs.rs 本批划 bravo——顺修 R12 既有 clippy 5 错（nonminimal_bool+cloned_ref_to_slice_refs×3+useless_format，最小机械改——f796077）。**德摩根改写经 Alpha 复核语义等价**（`!(A && !keep) ≡ !A || keep`）
- HEAD 既有 fmt 债 14 处（collection_commands/action_extractor/collection/task_projection，Alpha 域）→ 81af86f 全仓收口（纯格式零语义）——CI Rustfmt 门 f796077 红→81af86f 绿（清偿链自洽）
- 推送撞车一次（Alpha R17-α2 批并行）——rebase 后三件套复验再推

## 边界偏差披露
任务书预期 Rating 已有 Serialize derive——实测无。取快照手写序列化惯例（rating as_u32），core 零 serde 侵入，语义等价。

## 遗留（移交 Alpha）
- [x] **R-β2 已清偿（α3）**：cmd_review_due_count 裸 usize vs 前端 {count} 解构→徽标永不 hot——后端包 `{count}`（serde_json::json!，L1100）
- [ ] desktop-check CI 双 commit 红——OOM 定案（rustc ICE，runner 15G 内存挤出）——非代码债；workflow 并发/runner 尺寸调整属 Alpha 域
- [x] Alpha 复核补强（α3 挂载接线）：面板接 aurora:review-dirty 重拉（评分/建卡后统计不 stale）
