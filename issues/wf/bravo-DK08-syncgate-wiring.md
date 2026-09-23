# Bravo 任务书：SyncGate 接线（DK-08 §7.3 收官切片）

> 发起：Alpha · 2026-09-23
> 接收：Bravo（领地 **crates/aurora-sync/**\*\*）
> 依赖：`sync_gate.rs` 策略引擎（859a90c）+ mobile-ffi `AndroidNetworkState`
> 平台源（ae75ab1）+ `NetworkStateRelay` Java 接线（e79e6e4）——**全链已就绪**
> 性质：「仅 Wi-Fi 同步」策略侧接线。改造量约 1.5–2 人日（含测试）。

## 1. 背景与现状

`aurora-sync` 目前是**纯库**：引擎 / SyncRouter / OfflineQueue / SyncGate
各自测试全绿，但生产装配层（bootstrap / mobile-ffi / desktop）**尚未引用
SyncRouter**——同步执行链路整体未通。本切片只接 **sync crate 内部**的
gate→router→offline_queue 链；生产装配（构造注入 / 设置存储 / 设置页 UI）
**归 Alpha 后续切片**（见 §5 边界）。

## 2. 交付清单（全部在 crates/aurora-sync/** 内）

### 2.1 SyncGate 前置检查接入 SyncRouter

- 主执行入口 `SyncRouter::route_and_execute`（router.rs:413）与
  `sync_via_route`（:372）在**发起传输前**调用 `SyncGate::evaluate`；
- `GateDecision::Allow` → 原路径不变；
- `DeferUntilUnmetered` / `DeferUntilOnline` → **不发起传输、不计失败、
  不触发熔断**，转 OfflineQueue（`enqueue`）延迟重试。语义锚点：
  策略拒绝≠故障（sync_gate.rs 模块文档已定调，勿污染重试/熔断统计）；
- 实现形态自由（`with_gate` 构造 / 独立执行封装均可），但：
  - `SyncGate` 可选注入——未注入时行为与现状完全一致（桌面默认放行
    不回归的代码级保证）；
  - 入队项保留 `idempotency_key`（幂等重放）。

### 2.2 OfflineQueue 恢复语义

- 网络恢复（`current_class` 回到 Unmetered）后的重放路径：出队 → 重执行
  → 成功移除 / 仍 Defer 保留（或按既有 compress_batch 语义聚合）；
- 重放不绕过 gate（同样走 evaluate）。

### 2.3 测试（DST 模式，FakeClock 既有基建）

- [ ] `wifi_only=ON` + Metered → 传输未发起 + 队列入队 + 熔断统计不变；
- [ ] Offline → DeferUntilOnline → 同上；
- [ ] `set_wifi_only(false)` 运行时切换 → 立即 Allow（既有 4 单测的集成版）；
- [ ] 恢复重放：入队 → 网络转 Unmetered → 重放成功出队；
- [ ] 未注入 gate 的 Router 全部既有测试不回归。

## 3. 验收（Alpha）

1. `cargo test -p aurora-sync` 全绿 + clippy -D warnings + fmt；
2. gate 未注入路径零行为差异（代码评审 + 既有测试回归）；
3. 领地核查：diff 仅 `crates/aurora-sync/**`（Cargo.lock 例外）；
4. 分支 `wf/bravo-syncgate-wiring`，进度按切片 commit。

## 4. 已就绪依赖（无需动作）

- `NetworkClass` / `NetworkStateProvider` / `SyncGate::evaluate` 语义
  （sync_gate.rs 859a90c，测试 4 项）；
- 平台源：Android `AndroidNetworkState`（AtomicU8 缓存 + 越界归零，
  ae75ab1）+ `NetworkStateRelay` Java 回调（e79e6e4，三态 0/1/2）；
- iOS `NWPathMonitor` **不在本切片**（DK-08 iOS 切片另行排期）。

## 5. 边界（越界即 request）

| 项 | 归属 |
|---|---|
| sync crate 内 gate/router/queue 接线 + 测试 | **Bravo** |
| bootstrap 生产装配（SyncRouter/OfflineQueue/SyncGate 构造注入） | Alpha 后续 |
| `wifi_only` 设置存储接口 + 设置页 UI（desktop/mobile） | Alpha 后续 |
| mobile-ffi / desktop command 层改动 | Alpha |
| 桌面 netwatch `is_expensive` 平台源 | 暂缓（裁决 09-20：默认放行） |

需要 bootstrap 暴露构造参数 / 设置存储 trait 时 → request 文档
（issues/wf/bravo-request-*.md），不预写 Alpha 领地代码。

## 6. 提交约定

- 分支：`wf/bravo-syncgate-wiring`（自 origin/main 最新切出）；
- commit 前缀 `feat(DK-08):` / `test(DK-08):`；
- fixture/资源 <10KB；单 commit 语义完整；
- push 前自查 `origin/main..HEAD` 只含本切片文件。
