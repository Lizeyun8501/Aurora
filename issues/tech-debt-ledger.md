# Aurora 技术债台账（独立档 · 2026-10-14 对账版 → 2026-11-24 O-05 增补）

**对账**: Alpha 2026-10-14 17:53 ｜ **主档前身**: DK-14 前置勘察拆卡序列 §4 ｜ **方法**: 逐项代码现状核查（非注记转抄）｜ **结论**: T1-T8 中 **5 项已随各卡消化**，本档为唯一权威台账（此前散落注记，不再逐注记更新）

**状态语义（O-05 / 2026-11-24 起）**：每条债显式标 OPEN 或 CLOSED——
CLOSED 条目必须附 commit 锚点；OPEN 条目必须附来源锚点（代码行/卡号/ADR）。
新增债直接入档；修复后原地改 CLOSED 不删除（历史可溯）。

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
| T4 | 桌面单条队列 drain/重试（现仅全量） | DK-48 | B-06 已消化（cmd_queue_retry+drain_one_doc_sync 既有;B-06 补 attempts 失败落库 bump_attempts） | 199b1ac |
| T6 | 同步事件实时推送（现 15s 轮询） | DK-48 | B-06 已消化（SyncStateChanged Low 事件+listener 桥+Badge 三层实时化,15s 兜底保留） | 199b1ac |
| T8 | **bench 基建**（与 RV-01 合流为一项：index-rebuild 基准，DoD 1k 重建<5s，首跑实证 20.4s=20ms/条；bench.yml 周跑或本地 --ignored） | 多卡挂账+DK03S3 | **B-08 已消化**：周跑 6 连周 schedule success（10-05→11-09）+artifact 90 天保留实证（run 42606165725）+回归门禁 step 新增（>10% warn-only 非阻断） | B-08 199b1ac 后续:本次 2026-11-15 |
| C-① | ~~CapabilityDenied token-None 占位语义~~ **关账（B-07 2026-11-15）** | 14c.1 | — | 全路径核验：init/handle-event token-None→denied+审计行（wasm_runtime.rs:949/986）;guest 面 table.get miss→granted=false→CapabilityDenied——无 None 假装通过路径 |
| C-ns | ~~per-plugin namespace（L1 数据隔离，doc-id 强制前缀）~~ **关账（B-07 2026-11-15）** | 14c.3 设计稿 | — | DK-16 L1 已落（storage_guard 第二环 `{id}:` 前缀+CapabilityDenied+query 键集过滤+share 授权面）;B-07 补 legacy 无前缀只读回退一版+host-invoked 容量 guard 审计行;前缀形 `{id}:` 保持（share split-once 语义依赖——工单 `plugin:{id}:` 形不采用,披露） |

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

## A-03 勘察登记（2026-11-11 Bravo）
- [基线既有红] moveBlock.test.ts 两失败（[UL,BQ] swap 多出 PARAGRAPH）——8f1380a clean stash 对照实证;vitest 环境今日修复（root @testing-library/dom 声明未安装——npm install 补齐）后历史不可见债现形;非 A-03 diff 面（moveBlock 纯 PM 事务,无 md 解析）
- [环境修复] vitest 全库可跑（92/94 绿）——dom 包装版 16.3.2 peer 需 dom@10,今日装齐
- [v1 边界] 源码态协同编辑不做（停手边界）;CM6 ~350KB gzip 本地打包（codemirror meta+lang-markdown,零 CDN）

## V26 批次一登记（2026-11-24 · O-05 收编）

### OPEN（11 项）

| # | 债 | 来源锚点 | 对应卡/时机 |
|---|---|---|---|
| V26-01 | KV seal 债 ×7：event/cal/task/dbview/bookmark/theme/liq 用户内容明文落库 | `kv_namespace_registry.rs` debt 字段（commit 9fefe6c，债快照测试锁定） | seal 改造专卡（O-02 停手边界明确另卡） |
| V26-02 | 桌面 WriteContext.content_cipher=None ×3（lib.rs:132/1727、ai_commands.rs:123）——桌面笔记面明文（移动同口径,双端加密语义未接 vault） | `apps/desktop/src-tauri/src/lib.rs` TODO(DK-07) | DK-07 安全卡重启时 |
| V26-03 | 桌面系统能力五占位：托盘/全局快捷键/菜单/剪贴板×2/通知（lib.rs:2445-2471 接口就绪+日志占位） | `apps/desktop/src-tauri/src/lib.rs` | 桌面体验批次 |
| V26-04 | wasm IPC session 鉴权挂点（wasm_runtime.rs:1082 ctx 未用） | `aurora-plugin/src/wasm_runtime.rs` TODO(L2+) | L2 权限模型卡 |
| V26-05 | ai.cloud_api_key 过渡态扁平存（生产应 secret store） | `system_settings.rs:720/817`（sealed 形态已迁移,过渡文案待清） | 安全批次 |
| V26-06 | C-13 尾：NoteEditor 附件面挂接（stash 未提交）+E2E mock 三类 | USER.md 在途记录（React #300 卡点,基线隔离证实非 C-13 引入） | C-13 收口卡 |
| V26-07 | tauri-plugin-shell deprecated→opener 迁移 | A-02 登记（2026-11-10） | A-02 后续 |
| V26-08 | PDF 内置渲染 pdfjs-dist 动态 chunk（~900KB gzip）双端 WebView 兼容性未验 | A-02 v2 建议 | 附件体验批次 |
| V26-09 | 桌面周视图列卡式（非 24h 像素轴,移动端等价增强） | A-08 登记 | F-06（24h 轴周视图） |
| V26-10 | 移动日历切读 VM：FFI 通道全通但 CalendarView 仍读 adapter 旧源 | A-08 登记（铁律 6 三段式:双写已落,切读=下卡） | F-07 |
| V26-11 | TS6133 unused ×2（RichEditor MARK_BTNS/auroraMarkdown labels） | A-01/A-08 登记 | **已闭环**——O-09 df26127 typecheck 5 红清零含此 2 处（待 O-12 验收引用）→ **CLOSED df26127/49eff1f** |

### CLOSED（批次一新增）

| # | 债 | 关账锚点 |
|---|---|---|
| V26-11 | TS6133 ×2 | df26127（O-09 typecheck 5 红清零）+49eff1f（ts-gate CI 硬闸防回归） |
| — | moveBlock.test.ts 2 失败疑债 | A-03 勘察证伪（O-09 实测 144/144 全绿,文档声称的失败不存在） |
