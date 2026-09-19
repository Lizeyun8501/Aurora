# Bravo Request：移动端「仅 Wi-Fi 同步」平台网络状态源（DK-08 §7.3）

> 发起：Bravo（领地 crates/aurora-sync/**）· 2026-09-19
> 接收：Alpha（mobile-ffi / apps / bootstrap / 集成主权）
> 性质：**平台接口申请**（任务书 §7.3 明示「需 Alpha 接口，写 request 文档」）
> 同步侧已交付：`crates/aurora-sync/src/sync_gate.rs`（策略引擎 + 平台源 trait，测试全绿）

## 背景与现状

「仅 Wi-Fi 同步」需要感知平台网络状态（是否计量/是否有网）。Rust 侧无法
直接访问 Android `ConnectivityManager` / iOS `NWPathMonitor`，必须由
mobile-ffi 注入。

**Bravo 已交付**（`aurora_sync::sync_gate`，随 wf/bravo-sync 分支）：

```rust
pub enum NetworkClass { Unmetered, Metered, Offline }

pub trait NetworkStateProvider: Send + Sync {
    fn current_class(&self) -> NetworkClass;   // 读缓存值，廉价
}

pub struct SyncGate { /* wifi_only: AtomicBool + provider */ }
impl SyncGate {
    pub fn new(provider: Arc<dyn NetworkStateProvider>, wifi_only: bool) -> Self;
    pub fn set_wifi_only(&self, on: bool);     // 设置页运行时切换
    pub fn evaluate(&self) -> GateDecision;    // Allow | DeferUntilUnmetered | DeferUntilOnline
}
```

策略语义：`Offline` 恒推迟（与开关无关）；「仅 Wi-Fi」开启时 `Metered`
推迟；拒绝**不是错误**（调用方转 offline_queue 延迟重试，不污染熔断统计）。

## 申请项（Alpha）

### 1. mobile-ffi：实现 `NetworkStateProvider`（P1）

- **Android**：`ConnectivityManager.registerNetworkCallback` +
  `NET_CAPABILITY_NOT_METERED` 判定 → 缓存最新分类 →
  `jni` 回调/轮询暴露给 Rust；
- **iOS**：`NWPathMonitor`，`path.isExpensive` / `path.status` →
  同上（Swift ↔ FFI）；
- 同步阻塞点：只需 `fn current_class(&self) -> NetworkClass`（同步、无阻塞、
  读缓存即可；变更推送非必需——每次同步发起时轮询已够）。

### 2. 桌面端（P2，可默认放行）

工作区已内置 `third_party/netwatch`（iroh 依赖，含 android/windows/bsd/linux
后端）：`netmon::Monitor::interface_state()` → `State::is_expensive`
（LTE vs Wi-Fi）。**注意**：netwatch 文档明示 `is_expensive`
"not populated by `get_state` on all OSes"——请 Alpha 核实各桌面平台填充
情况；未填充时默认 `Unmetered` + 产品上桌面端默认关闭仅 Wi-Fi。

### 3. 设置存储与装配（P1，随平台源一起）

- `wifi_only` 用户设置的存储位置与默认值（建议：默认 OFF，`apps` 设置页 + 存储归 Alpha）；
- bootstrap 装配点：构造 `SyncGate`（注入平台 provider）→
  传入 SyncRouter / 各引擎入口；
- **Bravo 待办**（接口就位后我在领地内完成）：`SyncRouter::execute` 与
  `cloud/p2p/lan` 引擎入口接入门检查，`Defer*` 转 offline_queue 重试
  （已在 sync_gate.rs 模块文档标注接线方式）。

## 不受阻塞的部分（已完成，无需动作）

- 策略引擎 + 单元测试（4 个，覆盖开关/分类全组合、运行时切换即时生效）；
- 类型通过 `aurora_sync::sync_gate::*` 与顶层 re-export 均可访问；
- mobile-ffi 依赖 aurora-sync 即可实现 trait，**无需改 aurora-core**
  （trait 定义在 sync crate，平台源从上游注入，与 DK-00 冻结无冲突）。

## 验收清单（Alpha 接口落地时）

- [ ] Android/iOS provider 单元覆盖（计量/不计量/离线三态映射正确）；
- [ ] `wifi_only=ON` + 蜂窝 → 同步请求不发出（端到端）；
- [ ] 网络切换回 Wi-Fi → offline_queue 中被推迟的任务恢复执行；
- [ ] 桌面端默认行为不回归（gate 默认 OFF 或 provider 默认 Unmetered）。
