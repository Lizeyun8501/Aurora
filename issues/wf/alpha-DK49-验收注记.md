# DK-49 Alpha 验收注记（关账）

**验收**: Alpha 2026-10-11 20:25 ｜**结论**: ✅ 通过关账 ｜**commit**: 2cea82c（单发一次成型，零 CI 修复红）

## 终证渠道

- **CI 五检查全绿**（2cea82c）：Test / MSRV / desktop-check / Clippy / Rustfmt

## DoD 六条核对

| DoD | 结论 | 依据 |
|---|---|---|
| 1. 7 函数全链贯通 | ✅ | 落点矩阵五列全✓：Rust impl→JNI 导出→Java 声明→AndroidBridge→androidPlatform（工单"三层穿链"假设被勘察纠正，见亮点 1） |
| 2. 桥落点文档 | ✅ | bravo-report-DK49-bridge-landing.md 75 行：活通路两层直连图+cpp 残留层披露+bridge.ts 平走披露+7 函数矩阵 |
| 3. mock 对齐 | ✅ | androidPlatform 7 调用 mockSyncCalls 录制回放+localStorage 降级（浏览器开发预览不依赖真机） |
| 4. UI 零变更 | ✅ | MobileApp.tsx + apps/desktop 零 diff；改动面全在工单解禁清单内 |
| 5. 单测 | ✅ | mobile-ffi lib.rs 20 处 test；CI Test 稳定绿 |
| 6. CI 五门 | ✅ | 全绿 |

## 亮点

1. **纠错勘察不将错就错**：工单假设"三层穿链（Rust→aurora_jni.cpp→Java）"，Bravo 勘察证实 cpp 层**不参与构建**（CMakeLists 空壳、extern "C" 声明 Rust 侧零命中）——实际两层直连，按真实通路实施且在报告首节披露差异。§0"仍需自行复核"条款起效。
2. **停手边界实战命中**：工单停手第 2 条（bridge.ts 未通 native）属实命中——按边界执行 FFI→Java 照做+mock 对齐+如实记录，未擅自扩范围改桥架构。
3. **装配=移动端自己的范式**：Option<Arc> 四件（SyncGate/OfflineQueue/ConflictArtifactStore/ConflictResolver）挂 UniffiAppCore——沿用 DK-44"Option 渐进启用"思想但落位移动端自有装配，未照搬 BootedApp；另加 attach/detachSyncEngine 生命周期注入辅助（工单表外新增，合理并已披露）。
4. **network_state.rs 纪律**：+6 行纯可见性放宽（private→pub(crate)）+用途注释——语义零改。
5. **drain feature gate**：cfg p2p-sync 双面导出处理，无 p2p 构建面不破。

## 技术债登记（DK-50+ 候选）

- cpp 残留层清理（aurora_jni.cpp 与 Rust 同名符号重复，因不编译未爆雷）——建议清理或注明历史地位
- 队列落盘/持久化（本卡内存态挂载，按停手边界记录）
- drain 的 wifi_only 联动网络策略（移动端 network_state 现有面已用，缺的按桌面语义记录）

## DK-50 解锁

依赖已满足（adapters 7 调用面就绪）——三组件移植开工令可发（MobileApp 2071 行壳膨胀红线届时设置）。
