# DK-28 开工令 — completed_at 事件流回填 + tags 投影启动成本核验（Bravo）

> 发令 Alpha 2026-10-04 · 领地：aurora-core 投影层（Bravo）· 预算 0.5-1 人日

## 背景

DK-25 诚实化注记「completed_at 为投影内存态，存量数据首次完成才有时间戳」——存量任务周回顾样本失真；DK-27 引入 tags 投影启动全量重建（KV 全量扫）——成本未量化。

## 卡面两任务

1. **completed_at 事件流回填**：投影重建路径重放 TaskStatusChanged 历史（终态 stamp/回退清 None 与在线路径同口径）——存量任务 completed_at 激活；
2. **tags 投影启动成本实测（数据决策卡）**：10k 笔记量级 KV 全量扫 bench（release 口径）——**<100ms 文档化免优化**；超标则增量方案评估（不做无数据优化）。

## DoD

1. 回填 parity 测试（历史重放 vs 在线 stamp 终态一致——含回退非终态清 None 链）；
2. bench 数字落交付报告（口径注 release，对照 perf-baseline.md 口径惯例）+ 数据决策结论；
3. CI 四门；领地最小补丁声明。

## 守则

注入时钟 / 测试住本 crate / DEV_DEBUG=0 / **unused import 门**（Weekday 两犯——commit 前 `cargo clippy --all-targets -D warnings` 必跑）/ 四门自证。

— Alpha 2026-10-04
