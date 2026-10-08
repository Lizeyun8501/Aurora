# DK-39 交付报告：WASM 插件沙箱资源限制（epoch + limiter）

> 执行：Bravo 2026-10-07 · 依据：alpha-DK39-开工令（DK-33 验收风险注记 c6778ac 硬前置）
> Commit：见 git log（feat(DK-39)）

## 落地面

### 1. epoch interruption（CPU 面，开工令方案 A 主路径）
- `WasmtimeRuntime::new()`：`Config::epoch_interruption(true)` + 后台 tick 线程
  （10ms `increment_epoch`，官方推荐模式；desktop 进程常驻单例，线程随进程存活）
- per-call `store.set_epoch_deadline(n)`——超期 trap（`Trap::Interrupt`）→
  `Error::Internal("plugin deadline exceeded")`

### 2. ResourceLimiter（内存面，开工令"本卡必做"）
- `LimiterState` 实现 `wasmtime::ResourceLimiter`：
  - `memory_growing`：desired 超上限 → 拒绝（Err trap）——缺省 64 MiB
  - `table_growing`：上限 10k 元素
- **实现注记**：wasmtime 36 将 limiter Err 包装为通用 trap（message 不透传
  downcast），故 LimiterState 置 `rejected_memory` 状态位，调用后
  `classify_call_err` 优先按状态位还原"内存超限"语义——
  `Error::Internal("plugin memory limit exceeded")`

### 3. manifest 可配（DoD 4）
- `PluginManifest.sandbox: SandboxLimits`（serde default 向后兼容旧清单）：
  - `epoch_deadline_ticks`（缺省 200 = 2s，tick=10ms）
  - `max_memory_bytes`（缺省 64 MiB）
- 零配置默认受限（安全缺省），manifest 显式覆盖

## DoD 验证矩阵

| DoD | 断言 | 结果 |
|---|---|---|
| 1. 死循环 deadline | `loop br 0` 样本（spin），manifest 配 20 ticks=200ms → "plugin deadline exceeded"，<2.5s 返回；调用后同 runtime answer 继续服务 | ✅ |
| 2. 内存超限 | grow 循环样本（64 pages/iter），缺省 64MiB → "plugin memory limit exceeded"；之后 answer 正常 | ✅ |
| 3. 正常插件回归 | DK-33 answer/bump 样本 2/2 绿（dk33_load_and_invoke_e2e / dk33_unload_and_missing_entry） | ✅ |
| 4. manifest 可配 | dk39_sandbox_limits_default_safe：缺省值断言 + 旧清单（无 sandbox 字段）serde 反序列化成功 | ✅ |
| 5. 四门 | TEST=0（全量 workspace，--exclude desktop）/ CLIPPY=0（default+loro-crdt 双 feature 组合，-D warnings）/ FMT=0 / DESK=0（CI 面 main-only，dev push 不触发——历史同款，本地门为准） | ✅ |

## 边界外（注记，未做）
- fuel 计费语义（后续卡，开工令明示）
- &str/String 跨界（DK-33 注记独立卡）
- 插件签名/来源校验（市场面独立卡）

## 过程坑（沉淀）
- wasmtime 36 `ResourceLimiter` 签名 = `usize`（非 u64/u32，手写前先编译探签名）
- limiter Err → 通用 trap：message 被 wasmtime 吞，downcast 拿不回——状态位还原语义
- `#[async_trait]` 属性必须紧贴 `impl PluginRuntime`（插块隔开 = 语法错）
- wat 样本导出函数须带 `(result i32)`（invoke 契约 ()->i32，无返回 = get_typed_func 转换失败）
