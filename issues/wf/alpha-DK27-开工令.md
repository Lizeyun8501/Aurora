# DK-27 开工令 — tags 投影 v2 + SmartFolder tags 条件（Bravo）

> 发令 Alpha 2026-10-04 · 领地：aurora-core 投影层 + bootstrap 评估面 + SmartFolderView UI（全链 Bravo）· 预算 1.5-2 人日

## 背景

DK-02-S3 挂账：SmartFolder 结构化过滤 tags 条件 v1 未做——tags 存 Loro doc meta **无轻量读取源**（诚实化）。**依赖已解锁**：DK-22 Projection.apply_batch 批量投影基建落地，新投影接入成本已降。

## 卡面三任务

1. **TagsProjection 轻量读取源**：tag↔note 映射维护（NoteCreated/更新/删除事件的 tag 变更捕获），参照 SearchIndexProjection/TaskProjection 模式；**apply_batch 覆写或默认逐事件**——按 tag 变更批量特征数据决策（DK-22 先例：占比数据裁决，不为攒批而攒批）；
2. **FilterRule tags 条件 v2**：evaluate 接 TagsProjection（零向量路触碰、零 SearchBackend 改动——S3 领地声明延续）；
3. **UI 接线**：SmartFolderView tags 条件编辑（多选/排除语义与 v1 rule 字段向后兼容）。

## DoD

1. 行为级测试住本 crate：tag 增删改投影一致性 / tags 条件过滤正确性 / 批量重建 parity（apply_batch 与逐事件终态一致）；
2. tsc 0 + vite 0 + CI 四门（DESK 假 pc 路径）；
3. **存量兼容诚实化注记**（completed_at 先例）：存量笔记 tags 首次变更前投影态口径写明；
4. serde 向后兼容（FilterRule 新字段 default）。

## 教训继承

全套（注入时钟守则 / 测试住本 crate / bench 场景真实性 / DEV_DEBUG=0 / 四门自证 / 领地最小补丁声明）。

— Alpha 2026-10-04
