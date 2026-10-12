# DK-51 Alpha 验收注记（关账）

**验收**: Alpha 2026-10-12 09:15 ｜ **结论**: ✅ 三案全关账，DoD 全达成 ｜ **commits**: d5b94ca（案一/T2）+ 77f6e3f（案二/T3）+ a1aac5c（案三/T1）——单发三连，每案独立 commit 照令

## 终证

- **CI 五门全绿**（a1aac5c 合并态）：Test / MSRV 1.91 / desktop-check / Clippy / Rustfmt
- Test 含新增：落盘 reopen 单测（案一）+ gate_matrix_all_combinations 全组合单测（案二）

## 案一（T2 队列落盘）DoD

| DoD | 结论 | 依据 |
|---|---|---|
| 队列落盘重启不丢 | ✅ | `OfflineQueue::open(path)` SQLite WAL，reopen 单测在 diff |
| 幂等键跨进程去重 | ✅ | UNIQUE 约束落库 + INSERT OR IGNORE，语义与内存版一致 |
| 桌面/移动功能不回归 | ✅ | CI 全绿；**消费端零改动**（diff 仅 lib.rs 装配 7 行 + offline_queue.rs）——接口签名零变佐证 |
| 依赖纪律 | ✅ | rusqlite 0.32（bundled+backup）入 workspace Cargo.toml 统一版本 |

设计加分：`new()` 保留为进程内 SQLite（测试用）、`open()` 落盘——同构双实现非残留，测试 32 处引用全部合法。

## 案二（T3 drain 过门）DoD

| DoD | 结论 | 依据 |
|---|---|---|
| wifi_only 拒发携 decision | ✅ | 桌面 cmd_queue_drain +9：evaluate→is_allowed→Err 携 `{decision:?}+reason()`；移动同款语义 |
| Allow 路径不回归 | ✅ | is_allowed=true 时行为与 DK-49/桌面现状一致 |
| evaluate 生产调用点 ≥2 | ✅ | 桌面 sync_commands.rs + 移动 mobile-ffi lib.rs |
| GateDecision 全覆盖单测 | ✅ | gate_matrix_all_combinations |
| **降级路径保持**（工单规约） | ✅ | 移动 `if let Some(gate)`——sync_gate=None 时跳过门禁不加隐形拦截，自注披露"降级模式已知且登记" |

语义灰区披露（注记提醒，不计偏差）：Defer 包装为 Err 返回——Tauri/MobileError 的 Result 形态约束所致，错误信息明确"同步被门禁推迟"非伪失败。后续 UI 迭代可为该 Err 加专用提示样式，本卡不动。

## 案三（T1 cpp 清除）DoD

| DoD | 结论 | 依据 |
|---|---|---|
| 两文件删除 | ✅ | 137 行纯删 |
| 构建面零残留 | ✅ | build.gradle 无 externalNativeBuild/cmake 悬空（删除前 rg 实证） |
| rg aurora_jni 零命中 | ✅ | 代码面零命中（issues 史档除外） |

挂接清单自查节：Bravo 报告含删除前 rg 实证披露——教训⑤执行到位。

## 流水线状态

T1/T2/T3 关账，技术债台账剩 T4（用户反馈驱动）/T5（冲突高频后）/T6（EventBus 成熟后）/T7（触碰该文件时顺手）/T8（待 token）——全部挂起条件明确，**无主动清债项**。

DK-14 序列全部就绪：14a 先行（开工令素材在预备稿+14d 报告修正案）→ 14d 三段（d1 Config 重构并 14b）。等 TA 拍板 14a 开工。
