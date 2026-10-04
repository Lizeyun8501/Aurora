# Alpha 验收回执 — Bravo DK-27 tags 投影 v2 + DK-25 CI 补核闭合

> 629f574（DK-27 本体+止血合一）· 2026-10-04 · 裁定 **通过** ✅

## 一、DK-27 验收三件套

### ① 代码实锤

| 开工令要求 | 实锤 |
|---|---|
| 任务1 TagsProjection | ✅ tags_projection.rs 304 行——tag↔note 映射（TagRow/水位线 `projection.watermark.tags`）；事件面三捕获（NoteCreated 空 seed / NoteMetadataChanged.tags 终态覆盖 / NoteDeleted 级联清理）；数据源 KV `note:{id}` NoteRecord.tags（serde default 兼容存量）；**数据决策：不覆写 apply_batch**（映射纯内存逐事件够用）——DK-22 数据决策惯例正确执行 |
| 任务2 FilterRule tags 条件 v2 | ✅ bootstrap 接线（全量数据源回调 thread::scope 扫 KV note: 前缀）+ evaluate 接投影（零向量路/零 SearchBackend 触碰——S3 领地声明延续） |
| 任务3 UI 接线 | ✅ SmartFolderView tags_include/exclude 逗号分隔多选/排除草稿（与 v1 rule 字段向后兼容） |
| DoD1 行为测试 | ✅ dk27_tags.rs 173 行 3 测（投影一致性/过滤正确性/serde 向后兼容）+ dk02_s1_trash 补字段后 15/15 |
| 止血（合一 commit） | ✅ FilterRule 字面量三处补 tags 字段（serde default 引入新字段的旧字面量适配）+ unused import 两行（Weekday 同款教训二次发生——**记录：dk 系列 unused import 教训未入卡面 DoD，转守则**）+ set_rule 参数 Option 化（tauri 宏上下文 serde attr 不可用——工程判断合理） |

### ② 本地复跑（Alpha 独立）

dk27_tags **3/3 绿（3.32s）** + dk02_s1_trash **15/15 绿（22.23s）**，EXIT27=0。

### ③ 时间线

629f574 **CI 五绿** + **2613126 DK-25 止血 CI 五绿补核成功**（上轮 API 异常挂账正式闭合）——双闭合。

### 诚实化采信

存量笔记首次标签变更前投影态为空集（doc 注记）——采信。**注意**：tags 投影数据源为 KV 全量回调（thread::scope 扫），启动路径成本待数据——转 DK-28 任务 2。

## 二、新任务：DK-28 开工令（派 Bravo，0.5-1 人日）

1. **completed_at 事件流回填**：投影重建路径从 TaskStatusChanged 历史重放恢复 completed_at（DK-25 诚实化注记的闭环——存量任务周回顾激活）；DoD：回填 parity 测试（历史重放 vs 在线 stamp 终态一致）；
2. **tags 投影启动全量重建成本实测**：DK-27 引入的启动开销（KV 全量扫 10k 笔记量级 bench）——**数据决策卡**：<100ms 文档化免优化；超标则增量方案评估（诚实化，不为优化而优化）；数字落交付报告（口径注 release）。

— Alpha 2026-10-04
