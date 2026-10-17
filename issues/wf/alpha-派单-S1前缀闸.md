# [Alpha→Bravo 派单] S1 前缀闸：storage_guard 保留前缀强制拦截

> 派单方: Alpha ｜ 日期: 2026-10-17 ｜ 前置: 用户拍板按挂起池序推进（①前缀闸②CallerCtx）——插件冻结池定向解冻
> 设计依据: docs/plugin-kv-capability-design.md §2/§34（保留前缀清单+强制闸设计已在案）

## 背景

插件系统冻结时挂起池固化首项。现状：宿主保留前缀（`plugins:`/`settings:`/`links:` 等）靠"插件 id 禁 `:`+前缀形不相交"**间接保证**——无显式强制闸。`links:` 已登记（S1 派单第 20 行+设计稿第 24 行）——键域持续扩容，间接保证需升级为硬闸。

## 任务

`storage_guard` 扩展——**HOST_RESERVED_PREFIXES 强制闸**：

1. **清单硬编码**：`plugins:`（installed/blob/modcache/audit/policy 全系）+ `settings:` + `links:`——模块级常量清单（设计稿 §2 表格为源，实施前 re-scan 复核键域）
2. **拦截语义**：插件侧 KV 写/删目标键命中任一保留前缀 → `denied`（reason=`reserved-prefix`）+ **审计行**——**即使键形与 `{id}:` 相交也拒**（插件 id 恰为 `plugins` 等边界形——16.1 禁 `:` 不禁此形，硬清单兜底）
3. **读面口径**：v1 拦写不拦读（读保留前缀=插件可见性策略，另批）——注记诚实边界
4. **插件 doc 键不受影响**：`{plugin_id}:*` 正常路径零回归

## 纪律

- core 插件存储面+capability 检查链——不动宿主自身 KV 路径（宿主特权不受闸）
- 单测：命中拒+审计行+边界形（id=`plugins`）+doc 键不误伤——四类覆盖
- CI 全绿+注记落档+直推 main（惯例）

## DoD

1. 四类单测全绿+既有 127 单元/22 e2e 零回归
2. 设计稿 §2 表格补充闸状态注记（从"间接保证"→"强制闸落地"）
3. 注记落档+直推
