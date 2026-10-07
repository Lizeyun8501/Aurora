# DK-42 交付报告：SandboxLimits 移植（TA 裁决折中方案落地）

> 执行：Bravo 2026-10-08 · 依据：alpha-SandboxLimits移植评估（295a323）TA 拍板
> ——同意 Alpha 折中：**维持 main 软拒绝语义 + 采 Bravo 触顶置位** + manifest 级可配 + 错误分类
> 前置：已切 main（dev 废弃重建——7d11a91 硬前置执行）
> Commit：见 git log（feat(DK-42)）

## 落地面（评估 §四建议 a+b+c 全项）

### 1. manifest 级可配（评估价值：高——插件市场刚需）
- `PluginManifest.sandbox: SandboxLimits`（serde `#[serde(default)]` 向后兼容：
  旧清单无字段反序列化即缺省安全值 2s/64MiB，零迁移）
- `SandboxLimits { epoch_deadline_ticks: u32, max_memory_bytes: usize }`（Copy）
- invoke 时 per-plugin 覆盖 runtime 缺省（`0` 值防御性回退 runtime 缺省）

### 2. 触顶置位（折中方案核心——软语义兼容 + 可观测补齐）
- `MemoryLimiter` 软拒绝语义不变（`Ok(false)` → guest `memory.grow` 返 -1）
- 拒绝同时置 `hit_limit` 旗标；invoke 后（含 Err 路径）经 `record_hit_limit`
  落 runtime 级 map + `tracing::warn!` 日志
- `WasmtimeRuntime::last_call_hit_limit(plugin_id)` 宿主程序化查询；
  正常调用后旗标自动复位（per-call 语义）

### 3. 错误分类（评估价值：中高——可观测性）
- `classify_call_err`：epoch trap（`Trap::Interrupt`）→
  `plugin deadline exceeded`（明确语义）；其余 → `wasm call: {e}`（通用包装）
- 宿主可程序化区分「插件超时」vs「插件 bug」

### 4. 不移植（按评估明示）
- 硬拒绝语义（TA 折中裁决维持 main 软拒绝，免状态位补偿与兼容包袱）

## 验证矩阵

| 测试 | 断言 | 结果 |
|---|---|---|
| dk42_manifest_hit_limit_observable | manifest 配 256KB→超限样本软拒绝报错+`last_call_hit_limit`=true；正常调用后复位 | ✅ |
| dk42_error_classification_deadline | 死循环+短 deadline→`plugin deadline exceeded`（不含 `wasm call:` 通用前缀） | ✅ |
| dk42_sandbox_serde_default_compat | 旧清单反序列化=default（2s/64MiB） | ✅ |
| DK-33 回归 | answer/bump 全绿（含 eat 软拒绝既有断言） | ✅ |

| 门 | 结果 |
|---|---|
| TEST（全量 workspace --exclude desktop） | 0 ✅ |
| CLIPPY | 0 ✅（workspace 全量 -D warnings） |
| FMT | 0 ✅ |
| DESK | CI main 面为准 |

## 说明
- 规模 S（评估预估 ~110 行）：实际 manifest 字段+类型 ~25 行、limiter/接线 ~60 行、测试 ~90 行
- 插件市场迭代（DK-14 面）前完成 ✅（评估触发时机要求）
