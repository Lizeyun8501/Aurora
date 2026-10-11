# DK-49 桥落点勘察报告（DoD2 交付物）

> 勘察人：Bravo · 2026-10-11 · 依据：bravo-task-DK49 §0「bridge.ts 的落点需勘察」

## 1. 实际活通路（与工单 §0 勘察的差异披露）

工单 §0 假设「三层穿链：mobile-ffi → aurora_jni.cpp → UniffiAppCore.java」。
**勘察结论：cpp 层不参与构建与调用，实际活通路为两层直连 + JS 侧两层**：

```
MobileApp.tsx (React)
  → androidPlatform.ts  platform.*（window.AndroidBridge 探测 + mock 降级）
  → MainActivity$AndroidBridge（@JavascriptInterface，MainActivity 内部类，非 Capacitor plugin）
  → UniffiAppCore.java（System.loadLibrary("aurora_mobile_ffi") + native 方法声明）
  → Rust Java_com_aurora_note_UniffiAppCore_nativeXxx（crates/mobile-ffi/src/lib.rs JNI 直导出）
  → UniffiAppCore impl → aurora-sync（SyncGate/OfflineQueue/ConflictArtifactStore/ConflictResolver）
```

**cpp 桥是残留文档层**：
- `cpp/CMakeLists.txt` 为空壳（注释：No CMake build needed — Rust .so 预编译直接放 jniLibs/）
- `aurora_jni.cpp` 里的 `extern "C" { aurora_native_new(...); }` 声明在 Rust 侧**不存在**（rg 全库零命中）；
  其 Java_com_* JNI 实现与 Rust 侧同名符号**重复**——因 cpp 不编译进 so 而未爆雷
- **处置**：cpp 文件本卡不动（无构建影响）；DK-50+ 建议清理或注明历史地位

**bridge.ts 是平行走**：Capacitor 插件抽象（MockBridgeBackend 录制模式），仅被
MobileApp 用于 Filesystem/Clipboard 等 @capacitor/* 插件调用，**与 AndroidBridge
同步通路无关**——7 调用未走此层（工单停手边界第 2 条命中：「bridge.ts 实际未通
native」）→ 本卡按边界执行：FFI→Java 照做 + mock 侧对齐 + 此处如实记录。

## 2. 7 函数落点矩阵（全链贯通 DoD1）

| # | 函数 | Rust impl | JNI 导出 | Java 声明 | AndroidBridge | androidPlatform |
|---|------|-----------|----------|-----------|----------------|-----------------|
| 1 | sync_status | `sync_status_impl` | `nativeSyncStatus` | ✅ | `syncStatus()` | `platform.syncStatus()` |
| 2 | queue_list | `queue_list_impl` | `nativeQueueList` | ✅ | `queueList()` | `platform.queueList()` |
| 3 | queue_drain | `queue_drain_impl`（cfg p2p-sync） | `nativeQueueDrain`（双面） | ✅ | `queueDrain()` | `platform.queueDrain()` |
| 4 | conflict_list | `conflict_list_impl` | `nativeConflictList` | ✅ | `conflictList()` | `platform.conflictList()` |
| 5 | conflict_resolve | `conflict_resolve_impl` | `nativeConflictResolve` | ✅ | `conflictResolve()` | `platform.conflictResolve()` |
| 6 | conflict_resolve_semantic | `conflict_resolve_semantic_impl` | `nativeConflictResolveSemantic` | ✅ | `conflictResolveSemantic()` | `platform.conflictResolveSemantic()` |
| 7 | wifi get/set（已有） | `wifi_only_impl`/`set_wifi_only_impl` | `nativeGet/SetWifiOnly` | ✅（既有） | `get/setWifiOnly()` | `platform.get/setWifiOnly()` |

装配辅助（非工单表内，engine 生命周期注入）：`nativeAttachSyncEngine` /
`nativeDetachSyncEngine` + `UniffiAppCore.attach/detachSyncEngine` +
`SyncEngine.engineHandle()`（Java getter）+ `p2p_sync.rs transport()`（Rust 只读访问器）。
MainActivity `ensureSyncEngine()` 统一四处 `SyncEngine.start()` 调用点并挂接；
`closeSyncEngine()` 同步 detach（防 use-after-close）。

## 3. 错误与 mock 语义（全链一致性）

- **Rust→Java**：结构化查询（status/list）失败返回 null/`"[]"`；业务前置错误（drain/resolve）
  `throw IllegalStateException` → AndroidBridge `catch (Throwable)` 降级：
  drain → `{"error": "<msg>"}`（照 syncNote 既有 error-json 惯例）、resolve → `false`
- **TS 层**：`platform.*` 桥异常全走 mock 内存态（与桥路径同形状返回）
- **mock（DoD3）**：`mockSyncState`（wifiOnly/queue/conflicts 内存态）+
  `mockSyncCalls`（method+args 录制数组，回放断言用）+ `setMockSyncState()`（场景注入）
  + `resetMockSync()`（测试隔离）；drain mock=消耗内存队列出报告、resolve mock=按 id 移除
- **feature 门控**：默认（无 p2p-sync）下 nativeQueueDrain 导出「P2P 功能未编译」显式错误、
  attach/detach 导出 no-op——Java 侧零探测，杜绝 UnsatisfiedLinkError

## 4. 停手边界执行记录

1. **队列落盘**：UniffiAppCore 挂载为内存态（BootedApp 同源提取，离线队列落盘随引擎
   生产切片）→ 按边界「先内存态挂载，缺口另立卡」执行
2. **bridge.ts 未通 native**：如上 §1，FFI+Java 层照做，全链贯通=FFI→Java→bridge(mock) 贯通
3. **drain 网络策略**：桌面 `cmd_queue_drain` 本身不含 wifi_only gate 判定（手动触发即同步，
   gate 判定留在 UI/调用方）——移动端照抄同语义，双端一致，无缺口；network_state.rs
   网络态源已由 bootstrap 装配进 SyncGate（DK-08 §7.3），gate.evaluate() 三态即时可用
4. **aurora-sync 核心语义**：零改动（仅内存态挂载消费既有 API + p2p_sync.rs 加只读访问器）

## 5. 顺手修复的存量破损（p2p-sync 门控面，CI 从未编译过）

1. `AndroidNetworkStateProxy` private → E0603 re-export 破损 → pub(crate)
2. p2p_sync.rs 冗余 re-export（network_state 四项 + unused imports）→ 清理，tests 直接 import
3. `AndroidNetworkState.cached` private → E0451 测试直构破损 → pub(crate)
   （p2p-sync --all-targets 本地验证通过；建议 Alpha 考虑 CI 补 p2p-sync feature 编译面）
