# Bravo · DK-25 交付报告 — 周回顾全周口径

**Base**: 3ee512d（DK-26 销账后）｜**执行者**: Bravo｜2026-10-04

---

## 一、卡面三任务落点

| # | 任务 | 落点 |
|---|---|---|
| 1 | completed_at 时间维度 | `TaskViewRow.completed_at: Option<i64>`（Unix ms）——`TaskStatusChanged` apply 处 stamp：转终态（done/cancelled，与 blocked 派生终态同口径）→ `Utc::now_ms`；转回非终态 → None（bootstrap source 拼装点同步补字段） |
| 2 | 周区间完成任务集查询 | `TaskProjection::completed_task_ids_between(from_ms, to_ms)`——**闭区间**（周一 00:00:00.000 含 / 周日 23:59:59.999 含），排序返回；与 `weekly_summary` 聚合衔接（tauri 层周计算，投影保持纯数据层） |
| 3 | 全链接线 | `cmd_weekly_review(week_start: Option<String>)`——YYYY-MM-DD 周一锚（非周一输入回退所在周周一；缺省本周）→ [周一 00:00, 次周一 00:00-1ms] → completed ids → weekly_summary；**「今日样本」标注移除**；前端 WeeklyReview **本周/上周切换** + range_label 显示 + 空周/无预估双空态 |

## 二、DoD 验证

| 项 | 证据 |
|---|---|
| 周边界行为级 | `dk25_week_boundary_inclusive`：周一 00:00:00.000 **含**（左闭）✓ / 周日 23:59:59.999 **含**（右闭）✓ / 下周一 00:00:00.000 **不含** ✓ |
| 跨月 | `dk25_cross_month_week`：2026-01-26~02-01 同周跨月，两侧均命中、次周界外不含 |
| 空周 | `dk25_empty_week_returns_empty_summary`：空 Vec + 汇总全 0 |
| stamp 链 | `dk25_completed_at_stamp_chain`（aurora-core 内联）：done → Some；回退 next → None |
| 前端 | **TSC=0 / VITE=0** |
| 四门 | 本地 `TEST/CLIPPY/FMT/DESK` 自证（见 commit）；CI run 为准 |

**4/4 测试绿**（bootstrap/tests/dk25_weekly.rs）。

## 三、面板描述（截图替代——无桌面运行环境）

今日视图任务列表下「周回顾」卡片：标题行右侧灰字显示周区间（如 `2026-01-05 ~ 2026-01-12`）；标题下「本周 / 上周」切换按钮（选中态主色高亮）；摘要行「完成任务 N 项 · 预计 X 分 · 实际 Y 分 · 偏差 Z%」（超时红/达标绿）；空周显示「（空周）」、有完成但未预估显示「（无预估样本）」；明细表三列：标题灰字 / 预估/实际 分（未估「—」）/ 偏差百分比（未估「未估」）。

## 四、领地与注记

- 触碰面：task_projection.rs（维度+查询+stamp）、bootstrap lib.rs（拼装点补 None）、tauri lib.rs（周参数）、DesktopShell.tsx（面板）——各自最小补丁
- completed_at 为投影内存态（KV 持久化归属后续持久化卡）；**历史任务无 completed_at（None）不进周口径**——诚实化：存量数据冷启动后首次完成才有时间戳
