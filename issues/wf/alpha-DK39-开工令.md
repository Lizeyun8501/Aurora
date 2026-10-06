# DK-39 开工令：WASM 插件沙箱资源限制（fuel/epoch）

> 派发：Alpha 2026-10-06 · 依据：DK-33 验收记录风险注记（c6778ac）——**插件市场开放前硬前置**
> 派发对象：Bravo（主力）

## 背景与问题

DK-33 闭环后插件可被编译与调用，但 wasmtime 未配资源限制：
- 恶意/缺陷插件**死循环将永久阻塞调用线程**（`TypedFunc::call` 无限时等待）
- 内存无上限（`Store` limiter 未配）——恶意插件可耗尽 host 内存
- 插件市场（未来）= 不可信代码执行面，当前**信任边界缺失**

## 技术方案建议（Bravo 可复核后择一/组合）

### 方案 A：epoch interruption（推荐主路径）
- `Config::epoch_interruption(true)` + 后台线程 `engine.increment_epoch()`（固定 tick，如 10ms）
- per-call `store.set_epoch_deadline(n)`——超期 trap（`Trap::Interrupt`）→ `Error::Internal("plugin deadline exceeded")`
- 优点：async 友好、编译期无变更、tick 全局共享（Engine 是 DK-31/33 共享单例——increment_epoch Engine 级 API 天然适配）
- 注意：共享 Engine 下 increment_epoch 影响所有 Store——deadline 按 tick 数换算，各插件独立 set

### 方案 B：fuel metering
- `Config::consume_fuel(true)` + per-call `store.set_fuel(n)`，call 后 `fuel_consumed()` 回读
- 优点：精确计量（可做计费/配额语义）
- 缺点：编译期 flag 全局生效（与共享 Engine 的其他用途耦合）、per-call 开销略高
- 定位：**后续卡**（市场配额/计费时再引入），本卡不强制

### 内存面（本卡必做）
- `Store::limiter()` 实现 `ResourceLimiter`：memory 上限（建议 64 MiB 起步，可 manifest 声明覆盖——permissions/config_schema 既有面复用）
- 超限 trap → 报错路径与 epoch 超时同构

## DoD

1. 死循环 wat 样本（`loop (br 0)`）invoke 在 deadline 内报错返回（≤2s），线程不阻塞（调用后 runtime 可继续服务其他插件）
2. 内存超限样本（grow 循环）报错返回
3. 正常插件（DK-33 样本 answer/bump）行为不变——2/2 回归绿
4. deadline/limiter 参数经 manifest 可配（缺省安全值，零配置默认受限）
5. 四门 TEST=0 CLIPPY=0 FMT=0 DESK=0（DESK 本地环境缺失以 CI 为准）

## 边界外（注记，不做）

- fuel 计费语义（后续卡）
- &str/String 跨界（DK-33 注记，独立卡）
- 插件签名/来源校验（市场面，独立卡）
