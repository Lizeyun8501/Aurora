# Bravo Task：DK-49 移动端同步数据面贯通（Rust+桥层，前置 DK-50）

> 发起：Alpha（TA 指令：移动端同款——DK-48 缺口移交第 4 项）· 2026-10-11
> 接收：Bravo
> 依据：桌面 DK-48 三组件已关账（461c0c6）；移动端同款的前提=移动端**同步状态面从零到一**。本卡=数据面链路贯通，UI 移植归 DK-50（另案排队，依赖本卡）。
> 状态：**开工令生效**。直推 main，CI 五门全绿后 Alpha 验收。

## 0. Alpha 勘察（Bravo 可直接引用，仍需自行复核关键点）

**移动端栈形态**：
- apps/mobile = Capacitor Android + WebView 前端（bridge.ts Capacitor 桥，mock fallback 录制模式）
- Rust→Java 三层穿链：`mobile-ffi`（JNI 导出）→ `aurora_jni.cpp`（C++ 中间层）→ `UniffiAppCore.java`（Java native 声明）——**每函数穿三层**，这是 FFI 面小的原因
- Android Java 侧：MainActivity.java / SyncEngine.java / UniffiAppCore.java；**无 Capacitor Kotlin Plugin**（@PluginMethod 零命中）——bridge.ts 的落点需勘察（案 1 交付物之一：写清桥落点）

**数据面现状缺口**：
- 底层 SyncQueue / ConflictArtifactStore 在 `crates/aurora-sync`（桌面 BootedApp 装配消费）；**移动端 UniffiAppCore 未挂载**（lib.rs 里的 queue 全是 review_queue 复习队列）
- FFI 同步面仅 SyncEngine 动作三件（nativeStart/LocalAddr/SyncNote）+ wifi 开关（nativeGet/SetWifiOnly 已有）
- 桌面对应物参照：apps/desktop/src-tauri/src/sync_commands.rs 的 8 command（本卡对齐其 7 个，见 §1）

## 1. 范围：数据面链路贯通（7 函数）

UniffiAppCore 挂载 SyncQueue/ConflictArtifactStore（**按移动端现有装配模式，禁止照搬桌面 BootedApp**）+ mobile-ffi 新增 native 函数并穿三层到 Java + adapters 层暴露：

| # | 函数 | 对齐桌面 | 返回 |
|---|---|---|---|
| 1 | native_sync_status | cmd_sync_status | JSON：gate/wifi_only/queue_len/pending_artifacts/pending_semantic |
| 2 | native_queue_list | cmd_queue_list | JSON 数组：QueueItemDto 同构 |
| 3 | native_queue_drain | cmd_queue_drain | JSON：DrainReportDto 同构（**failed_docs 不可丢**） |
| 4 | native_conflict_list | cmd_conflict_list | JSON 数组：ConflictDto 同构 |
| 5 | native_conflict_resolve | cmd_conflict_resolve（副本留存） | void/err |
| 6 | native_conflict_resolve_semantic | cmd_conflict_resolve_semantic（最新写入优先） | void/err |
| 7 | （已有）native_get/set_wifi_only | — | 复用不重写 |

- DTO 序列化**照现有 FFI 返回模式**（先读现有函数怎么返回结构化数据，同款来）
- bridge.ts / androidPlatform.ts 暴露 7 调用 + **mock backend 同步对齐**（录制模式可回放）
- gate 判定复用桌面同款三态语义（DeferUntilUnmetered 等），移动端网络态源=network_state.rs 现有面

## 2. DoD

1. **7 函数全链贯通**：Rust 导出→cpp 桥→Java→bridge.ts→adapters，任一环节缺失不算通
2. **桥落点文档**：bridge.ts 实际落到 native 的机制写清（写入本卡交付报告或 TOOLS.md 附注）——Alpha 验收要看
3. **mock 对齐**：mock backend 可录制/回放 7 调用（前端开发不依赖真机）
4. **移动前端 UI 零变更**：MobileApp.tsx 不动（UI 移植是 DK-50）；diff 允许面=mobile-ffi / aurora_jni.cpp / Java 三件 / bridge.ts / androidPlatform.ts / （如需）aurora-sync 装配辅助
5. **单测**：Rust 侧 7 函数可测即测（DTO 序列化/装配幂等）；桥层有现成测试基建则对齐
6. **CI 五门全绿**

## 3. 解禁 / 禁触

**解禁**：crates/mobile-ffi/**、apps/mobile/android/app/src/main/{java,cpp}/**、apps/mobile/src/adapters/**、（如需）UniffiAppCore 装配位所在的 crates 面。
**禁触**：
- apps/mobile/src/MobileApp.tsx 与移动端其余 UI（DK-50 领地）
- crates/aurora-sync 核心逻辑（只许装配/挂载，不改语义）
- crates/aurora-bootstrap、桌面全栈（apps/desktop/** 本卡零触碰）
- SyncEngine 既有三函数语义

## 4. 停手边界

- UniffiAppCore 装配 SyncQueue 需要**迁移/持久化面**（移动端队列落盘）→ 先做内存态挂载，落盘缺口记录另立卡
- 桥落点勘察发现 bridge.ts 实际未通 native（纯 mock 环境）→ 如实记录，FFI+Java 层照做，"全链贯通"降级为"FFI→Java 贯通+mock 侧对齐"，报告写明
- drain 需要网络策略判断（wifi_only 联动）在移动端缺失→ 按桌面语义实现，网络源用 network_state.rs，缺什么记录
- 任何需要动 aurora-sync 核心语义的情况 → 停手回报

## 5. DK-50 排队（依赖本卡，先不发）

三组件移植：SyncStatusBadge（badge 挂接点+轮询节流按移动端功耗惯例裁决）/ QueuePanel / ConflictReviewPanel——React 源码可从 apps/desktop/src/components/ 移植复用，移动交互适配另做；MobileApp.tsx 2071 行巨壳同样设膨胀红线（开工令届时另发）。

## 6. 规模

M-L（7 函数×三层穿链+装配+桥对齐；核心逻辑零改，风险在穿链完整性与装配模式）。
