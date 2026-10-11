# DK-14 插件生态前置勘察 + 拆卡序列（预备稿，待 TA 拍板）

> 勘察：Alpha 2026-10-11 深夜（等待 DK-50.1 交付窗口）· 只读勘察，不占 Bravo 领地
> 结论先行：**40 人日大卡可拆 5 卡流水线消化，其中 2 卡（a/b）无相互依赖可立即派发**

## 1. 基座现状（crates/aurora-plugin，3618 行）

| 面 | 现状 | 离 DK-14 距离 |
|---|---|---|
| WASM 运行时 | wasmtime 已挂（workspace 依赖），wasm_runtime.rs 576 行 | Component Model+WIT **缺**（现 core module 形态） |
| invoke 语义 | **伪成功实证**：L223/L242 硬编码 `{"ok": true}`；NotFound 仅用于"插件未注册"路径 | **近**——两处改语义化错误，最小独立卡 |
| Capability | permission.rs 526 行雏形（配合 DK-42 per-publisher 策略） | 中——缺 L0/L1/L2 三级模型+安装前可见 |
| Epoch/fuel | **零命中**——完全缺失 | 中——wasmtime epoch 机制现成，待接 |
| 市场/生命周期 | marketplace.rs 610 + lifecycle.rs 776 + hot_update.rs 466 | DK-42/43 已交付启用面，徽章 UI 待权限模型 |
| iframe 运行时 | iframe_runtime.rs 573 行 | 并存形态，本卡序列不动 |

## 2. 拆卡序列（依赖链：a、b 独立先行；c 依赖 a；d 依赖 a/b；e 收尾）

| 卡 | 内容 | 规模 | 依赖 | 风险 |
|---|---|---|---|---|
| **DK-14a** | **清除 invoke 伪成功**：未声明函数→`NotFound`、已声明未实现→`NotImplemented`（对齐 DK-14 DoD 原文）；审计日志记录越权调用 | S-M | 无 | 低——语义化错误替换，调用方（marketplace/lifecycle）回归面要盘 |
| **DK-14b** | **Epoch 中断**：wasmtime epoch interruption + 100ms tick + 最大 10 Epoch 超限拦截；超限→Capability 记录+插件标记 | M | 无 | 低——wasmtime 原生机制 |
| DK-14c | Capability L0/L1/L2 权限模型：默认零权限、安装前可见（marketplace 徽章 UI 面）、越权拦截+审计 | M-L | 14a | 中——permission.rs 雏形升级+UI 面 |
| DK-14d | Component Model+WIT 升级：语言无关接口 | L | 14a/14b | **高**——wasmtime 版本/构建链核查先行，单独勘察再定 |
| DK-14e | 官方示范插件 10-20 个（生态冷启动） | M | b/c/d | 低 |

## 3. 建议

- **14a 最优先**：伪成功是 DK-14 清单原话"比返回 Null 更隐蔽"的坑——安全修正，先清雷
- 14a/14b 可并行双卡（无领地冲突：同 crate 但文件面不重叠——a=wasm_runtime.rs invoke 路径，b=runtime 构造+调度）——如双开需工单明划文件领地
- 14d 高风险：开工令前置=单独勘察（wasmtime 版本、Component API 可用性、构建链影响），勘察报告先行再定卡

## 4. 附：技术债台账收敛（散在 DK-48/49/50 注记 → 本档统一）

| # | 债 | 来源 | 建议时机 |
|---|---|---|---|
| T1 | cpp 残留层清理（aurora_jni.cpp 与 Rust 同名符号重复，不编译未爆雷） | DK-49 | 与 14 序列错峰，XS 独立卡 |
| T2 | 移动端同步队列落盘（现为内存态） | DK-49 | 移动同步真实使用前必须 |
| T3 | drain 的 wifi_only 联动网络策略（移动端） | DK-49 | 随 T2 |
| T4 | 单条队列 drain/重试（桌面 cmd_queue_drain 仅全量） | DK-48 | 用户反馈驱动 |
| T5 | 冲突内容级 diff（扩 ConflictDto：local/remote 值或摘要） | DK-48 | 冲突高频后评估 |
| T6 | 同步事件实时推送（现 15s 轮询） | DK-48 | EventBus 桥成熟后 |
| T7 | 桌面 drain_to_peer_sync 双实现泛型化合并 | DK-44 注记 | 触碰该文件时顺手 |
| T8 | bench 落表 | 多卡挂账 | **待 token** |

## 5. 状态

预备稿**不派发**——等 DK-50.1 关账后 TA 拍板：①14a 单开 ②14a+14b 双开 ③先清 T1/T2 技术债 ④其他。
