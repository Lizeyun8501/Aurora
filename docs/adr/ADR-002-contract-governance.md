# ADR-002: 契约层冻结与变更治理

- **状态**: Accepted
- **日期**: 2026-09-11
- **关联**: V26-0 §4（契约层批次二）、DK-00、R-02、R-04

## 背景

V25 声称"契约冻结后三条流水线并行开工"，但 20 个端口只有名字、41 类事件字典与 62 个错误码在 V25 合并中全部丢失。契约是 11 个能力组件并行开发的前提——契约不定，它们只能等。

V26 DK-00 完成契约冻结：事件字典 41 类、端口 Trait 签名（★方法无默认实现）、CoreAPI TS 契约、62 错误码统一枚举、国际化契约期决策。

## 决定

### 1. 契约文件的唯一真源（Single Source of Truth）

| 契约 | Rust 真源 | TS 镜像 |
|---|---|---|
| 事件字典 41 类 | `crates/aurora-core/src/event_bus/event.rs`（`CoreEvent` + `CoreEvent::DICT`） | `shared/types/src/events.ts` |
| 错误码 62 个 | `crates/aurora-core/src/error_codes.rs`（`ErrorCode` + `TABLE`） | `shared/types/src/errors.ts`（待建） |
| CoreAPI | `crates/mobile-ffi/src/lib.rs`（JNI 基线） | `shared/core-api/src/index.ts` |
| 端口签名 | `crates/aurora-core/src/traits/*.rs` | —（Rust 单端） |

TS 镜像必须与 Rust 真源逐字段对齐；CI 编译两侧即为镜像校验。

### 2. 变更治理

- 契约的任何变更（新增事件/错误码/端口方法、修改签名/语义）**必须先提交 ADR**，禁止口头约定或顺带修改。
- ADR 模板最小集：背景 → 决定 → 影响（谁消费、谁需适配）→ 迁移步骤。
- 新增事件必须同步三处：`CoreEvent` 变体 + `CoreEvent::DICT` 行 + `events.ts` union 与 `EVENT_DICT` 行；新增错误码同步 `ErrorCode` 变体 + `TABLE` 行。
- 契约测试是守门员：`error_codes` 模块 7 个契约测试（62 计数/码往返/降级三原则）与事件 DICT 元数据查询随 CI 运行。

### 3. 编译期强制优于运行时约定

`SyncTarget::send_update / recv_update / sync_version` 三方法不提供默认实现（V26 §4.1.1 ★）。原因：远端默认实现让"增量同步在接口层成立、传输层不通"——三个适配器均未覆写，降级链路静默失效。编译期失败优于运行时静默返回空值。

### 4. 国际化契约期决策（R-04 → DK-00）

- **RTL**：布局不使用 left/right 绝对语义，一律 logical properties（start/end）；Tauri/WebView 层启用 `dir` 属性透传。
- **复数**：文案键一律带 ICU 复数参数（`{count, plural, ...}`），Rust 侧 `user_message()` 返回键而非拼接句。
- **日期格式**：VM 携带 ISO 8601，格式化由前端 locale 层负责；内核不产本地化字符串。

## 影响

- B/C 线（功能域卡）从此可并行开工——它们对契约只消费不定义。
- 违反契约的代码（未实现 ★ 方法、未登记的错误码）在 `cargo check` / `tsc` 阶段失败。
- 契约演进成本显式化：每次变更一篇 ADR，评审窗口即决策窗口。

## 验证

- `cargo check --workspace` 强制 ★ 方法全部实现（I1 已落地）。
- `cargo test -p aurora-core --lib error_codes` 契约测试 7 项全绿（I1 已落地）。
- `events.ts` 与 `event.rs` 的 DICT 行数相等（41），由 CI 两侧编译 + 后续镜像测试保障。
