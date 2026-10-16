# 功能完善批次：重装语义收口+审计面落盘（Alpha 2026-10-16，da9a570 后）

**目标重排**：用户指令——暂时不考虑市场，聚焦**软件功能完善+性能优化**。市场系任务（#3 trusted_keys/#7 市场后端/#8 权限授予链 listing 面/#9 更新检测）全部冻结。

## 交付一：重装语义收口（环 0）

**缺口**：install 对已存在 id 静默覆盖（KV set+register 直接换）——运行中插件被无声替换，无审计无确认，违 fail-closed 纪律。

**语义**：
- **环 0（重装检查，先于一切环）**：`plugins:installed:{id}` 已存在且 `reinstall=false` → `InvalidInput("already installed; pass reinstall=true …")`——升级语义显式归 hot_update 金丝雀卡
- `install_plugin`/`install_from_market` 签名加 `reinstall: bool`；命令层 `reinstall: Option<bool>` 缺省 false（UI 零改动即 fail-closed）
- 单测：首装成功/重复拒（"already installed" 断言）/reinstall=true 放行+KV 一致

## 交付二：审计面落盘（单键快照形态）

**缺口**：AuditRing 内存态重启即清——审计链无持久性。

**语义**（简单有界优先）：
- **单键快照**：`plugins:audit:snapshot` = bincode(Vec<AuditEntry>)，**SNAPSHOT_CAP=512 条**（容量天然有界，无增量键膨胀面，无删旧键成本）
- `persist_audit(runtime, kv)`：ring 快照→KV（取最近 512 条，旧→新序）
- `restore_audit(runtime, kv)`：启动回灌（**损坏 fail-open 返回 0**——审计是运维面，快照损坏不阻断启动；stderr 双写仍有 CI 留痕）
- **调用时机**：desktop 装配点 restore（restore_installed 旁）+ hourly tick flush（备份水位 tick 顺带——粒度=最多丢 1h 审计，v1 诚实边界）
- `AuditEntry` 加 Deserialize；`AuditRing.snapshot()/extend_history()`（append 语义+cap 丢最旧兜底）
- 单测：快照往返/回灌 cap 丢最旧/KV 往返/损坏 fail-open

## 搭车件

- fetcher nit：no-op truncate 死代码清理+unused_mut
- clippy too_many_arguments：`MarketInstallPolicy` 参数对象化（缺省=最保守 require_verified=true+reinstall=false）
- UI：`reinstall: false` 显式化（可读性）

## 验证

**122 单元（+3 重装/落盘系）+22 e2e 全绿，clippy 零告警**，全 guest 重建 SOP。lib.rs 块重组一处（plugin_state 定义前移到 tick 引用之前——desktop 面编译 CI 验证）。

## 性能优化线索（下批候选，待深挖）

1. **wasmtime 模块编译缓存**（M）：restore re-register 是 lazy（只登记 manifest 不实例化 ✓）但每次启动首个 invoke 仍要 wasmtime 编译 component——module serialization 落 KV 可跳过重编译（启动后首调延迟优化）
2. **installer KV 批量**：两次 set → batch_set（trait 已有，微收益）
3. **审计 flush 粒度**：hourly→定量（每 N 条）——持久性 vs 写放大权衡
