# Aurora 技术债台账（独立档 · 2026-10-14 对账版）

**对账**: Alpha 2026-10-14 17:53 ｜ **主档前身**: DK-14 前置勘察拆卡序列 §4 ｜ **方法**: 逐项代码现状核查（非注记转抄）｜ **结论**: T1-T8 中 **5 项已随各卡消化**，本档为唯一权威台账（此前散落注记，不再逐注记更新）

## ✅ 已消化（6 项——对账关账）

| # | 债 | 消化证据 |
|---|---|---|
| T1 | cpp 残留层清理（aurora_jni.cpp 与 Rust 同名符号重复） | `apps/mobile/android/app/src/main/cpp/` 目录已不存在 |
| T2 | 移动端同步队列落盘（内存态） | `offline_queue.rs:24`"已 rusqlite 真实化"（进程内 SQLite） |
| T3 | drain 的 wifi_only 联动网络策略 | `sync_gate.rs` NetworkStateProvider+wifi_only AtomicBool 实现 |
| T5 | 冲突内容级 diff（扩 ConflictDto） | `conflict.rs:51` `pub local_value: serde_json::Value` 在案 |
| T7 | drain_to_peer_sync 双实现泛型合并 | 全仓唯一定义 `drain.rs:49`，iroh_transport 复用（双实现已消） |
| T9 | host-invoked write-doc 容量 guard 对齐 | 2026-10-17 d4666a6：DefaultHostFunctions::WriteDoc 加 1 MiB 序列化长度 guard（HostFunction Err+stderr 侧通道）；审计行缺位为诚实边界；plugin lib 140+e2e 22 零回归 |

## ⬜ 在册（6 项）

| # | 债 | 来源 | 时机 | 备注 |
|---|---|---|---|---|
| T4 | 桌面单条队列 drain/重试（现仅全量） | DK-48 | 用户反馈驱动 | 无实现痕迹 |
| T6 | 同步事件实时推送（现 15s 轮询） | DK-48 | EventBus 桥成熟后 | 时机条件未到 |
| T8 | **bench 基建**（与 RV-01 合流为一项：index-rebuild 基准，DoD 1k 重建<5s，首跑实证 20.4s=20ms/条；bench.yml 周跑或本地 --ignored） | 多卡挂账+DK03S3 | **待 token** | 合流对账完成：T8=RV-01 同源，不再重复挂账 |
| C-① | CapabilityDenied token-None 占位语义 | 14c.1 | 与错误语义卡合流 | log/数据面路径已语义准确，仅 token-None 路径 |
| C-ns | per-plugin namespace（L1 数据隔离，doc-id 强制前缀） | 14c.3 设计稿 | L1 权限模型立项时 | 14c 系列收官候选首位 |

## 对账注记

1. **T8 与 RV-01 合流**：R03 回执+DK03S3 实证链（41ac132 首跑 20.4s）与 T8 bench 落表同源——统一为一项，挂"待 token"（bench artifact 产出需 token 预算）
2. **台账纪律**：本档唯一权威；注记中 T 项引用一律指回本档；新债直接入档不另立散页
3. 6 项消化均**非专卡清债**而是随功能卡顺手消化（错峰策略生效）——T4/T6 保持外部时机驱动，不主动排卡；T9 摘行系 2026-10-19 窗口对账（状态已在但挂错表）
