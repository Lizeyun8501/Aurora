# R18 留存率曲线任务书（Alpha 主做——Bravo 请认领其他卡）

> Alpha ｜ 2026-10-26 晨 ｜ 用户拍板「先清 CI→留存率曲线」连跑
> **域占用**：fsrs.rs 本批归 Alpha（留存聚合+测试）——Bravo 勿碰；ReviewStatsPanel 曲线段同批 Alpha

## 背景
R17 闭环：revlog（10_000 环形）+review_stats（今日截面）已就位。留存率曲线=按天时间序列延伸。

## T1 core（fsrs.rs，Alpha）
- `ReviewDayStat { date: String("YYYY-MM-DD"), reviews: usize, again_rate: Option<f64> }`
- `review_daily_series(now, days) -> Vec<ReviewDayStat>`——最近 days 天含今日（今日为末元素、未完诚实）；Utc 日界与 review_stats 同口径；当日 0 复习→again_rate=None
- 语义：只读 log 聚合（锁序 items 先 log 后同纪律——实际只需 log 锁，但保持同序读）
- 测试：多日聚合/日界边界/空日志/again 率分母（不含 None 早返语义——revlog 本就只记成功评分）

## T2 UI（ReviewStatsPanel.tsx 曲线段，Alpha）
- 最近 7 天柱状（纯 CSS 同 due_7d 模式）：柱高=复习量，hover title=日期+次数+again%
- again_rate>0.3 柱体染警示色（#F58323，克制小面积）；今日未完正常显示
- cmd 透传：cmd_review_daily_series（手写 Value 映射惯例）

## DoD
- cargo test -p aurora-core 全绿（478 基线+新增）
- tsc 0 + vite ✓
- 回执双发（Alpha 回执闭环）
