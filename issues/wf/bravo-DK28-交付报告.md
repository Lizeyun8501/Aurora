# Bravo · DK-28 交付报告 — completed_at 事件流回填 + tags 投影启动成本核验

**Base**: 9b1ae2c（DK-27 验收后）｜**执行者**: Bravo｜2026-10-04

---

## 一、completed_at 事件流回填（任务 1）

**缺口定位**：进程重启 → rows 内存重建（completed_at=None）→ 水位线不回退 → catch_up 不重放历史 → 存量 completed_at 永久丢失。

**修法（最小补丁）**：TaskProjection 增加 `with_status_history(StatusHistory)` 注入器（builder 式 &self，现有构造签名零破坏）——`rebuild()` 尾部重放历史序列，与在线 apply 同口径：终态（done/cancelled）stamp / 回退非终态清 None。bootstrap 装配注入：event_bus_store.events_after(0) 过滤 TaskStatusChanged（seq 升序）。

**口径注记**：历史事件无时间戳，重建路径 stamp=重建时刻（近似）；在线路径=真实完成时刻。终态图谱（Some/None）双路径一致——parity 测试锚定。

**DoD 1 parity 测试**：`dk28_rebuild_backfill_parity`（aurora-core 内联）——在线链 done→next→done + done→next（回退清 None）vs 重建路径同历史重放：**t-a 双路径均 Some / t-b 双路径均 None** ✅

## 二、tags 投影启动成本实测（任务 2 数据决策）

**口径**：release（bench.yml 惯例），SqliteStorage（生产 KV 实现）10k 笔记 NoteRecord JSON（30% 带标签近似真实分布），TagsProjection rebuild 全链（KV 全量扫+JSON 解析+映射构建）。测试：dk28_tags_bench（--ignored，row_count 断言全量入映射）。

| 项 | 数字 |
|---|---|
| **10k rebuild 总耗时** | **13ms** |
| 决策线 | <100ms 免优化 |
| **结论** | **13ms << 100ms → 文档化免优化，不做增量方案**（近 8 倍余量；成本由 KV 全量扫主导且随条数线性——10k 口径远未触线） |

## 三、验证

- dk28_rebuild_backfill_parity 1/1 绿；dk28_tags_bench release 1/1 绿（10k 断言）
- clippy --all-targets -D warnings 必跑（unused 门）+ 四门终态见 commit
- 领地：task_projection.rs（注入器+rebuild 回填）、bootstrap lib.rs（装配注入）、dk28_tags_bench.rs（新增）——最小补丁
