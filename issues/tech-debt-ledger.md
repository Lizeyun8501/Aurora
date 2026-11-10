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

## A-01 勘察登记（2026-11-09 Bravo）
- [他人既有] shared/ui-components RichEditor.tsx MARK_BTNS / auroraMarkdown.ts labels 两处 TS6133 unused（tsc 全库仅此 2 错;非本卡 diff 面——本卡改动零 tsc 错;归属 Alpha/前端面）
- [本卡注记] mobile 二进制导出链: FFI export_note_docx/export_collection_xlsx 返 Vec<u8> 已就绪,APK JS 桥 exportFile 现仅 text 通道——真机二进制落盘需 MainActivity 加 base64 通道（APK 重打不在本卡）
- [本卡注记] DOCX 真机 Word/WPS 开启验证不可行（沙箱无 Office）——OOXML 结构级测试已验（zip 规范+document.xml/styles 骨架+eastAsia 字体）;用户侧真机抽验建议保留

## A-08 (2026-11-10) 登记项
- **桌面周视图为列卡式**（7 列日卡+拖拽改期主通路完整），非移动端 24h 像素轴——24h 轴等价增强待后续卡（估 2 人日）
- **移动端日历切读 Rust VM**：通道已全通（FFI calendar_vm_json+Java 壳 calendarVm+TS bridge 接口），当前移动 CalendarView 仍读 adapter 旧源（铁律 6 三段式: 双写已落,切读=下卡）
- **既有 TS6133 ×2**（shared/ui-components RichEditor MARK_BTNS / auroraMarkdown labels,非本卡引入）——desktop typecheck 既有红点,单独小卡清
- **desktop browser-mock 日历空态**: CalendarPanel invoke=null 显示明示文案（非静默）——tauri 真模式全功能

## A-02 勘察登记（2026-11-10 Bravo）
- [环境既有] shared/ui-components vitest 环境坏: @testing-library/dom 缺失（Modal/BlockRenderer/Sidebar 既有测试同跑不动——非本卡引入）;AttachmentPreview.test.tsx 行为级断言已写就绪,环境修复即跑
- [本卡遗留] 移动端附件预览接线: MobileApp 无附件渲染宿主链（attachment:// 渲染/附件列表为移动端独立缺口）;AttachmentPreview 依赖注入设计已保双端可用;待移动端附件链落地即接（与 E25 APK 二进制桥同列）
- [v2 建议] PDF 内置渲染: pdfjs-dist 动态 chunk lazy-load（打包依赖非 CDN,~900KB gzip 不入主 bundle）——双端 WebView 动态 import 兼容性需先验;v1 按工单降级分支走系统打开
- [v2 迁移] tauri-plugin-shell open 已 deprecated——A-02 用 allow 保最小 diff,后续迁 tauri-plugin-opener（新依赖+capability 面评估）
