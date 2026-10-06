# Bravo · DK-40a 交付报告 — 混合存储 + 零丢失断言（V2：栈内 loro 方案）

**Base**: d2f5d89｜**执行者**: Bravo｜2026-10-06/07

---

## 一、裁决融合记录

- **用户裁决**：不接受 LWW 语义收缩 → CRDT 前置（块级+字符级零丢失）
- **V2 修正**（21ea11f）：**yrs 选型作废**——栈内核实 loro/NoteDoc（LoroTree 块树+LoroText）+ IrohTransport 已承载 CRDT 与传输面，引入 yrs = 第二 CRDT 栈纯负面；**40a 缩为混合存储 + 零丢失断言（大→中）**
- **执行遵守**：yrs 半成品（暂停期遗留）已全清；全程栈内 loro——**零第二栈**
- **教训吸收**：评估先 rg 代码（本轮 NoteDoc 1073 行考古先行，避免重蹈）

## 二、探明结论（开工令问题域回答）

| 问题 | 结论 |
|---|---|
| sea-orm 面 | **现实 = rusqlite 0.32 直用**（migration/BlockStore）——无 sea-orm |
| log 持久化形态 | **KV 键空间** `updatelog:{note_id}:{seq:020}`（零 schema 演进+与 notesnap 同生命周期）；SQL 表形态待 38b 实测后按需迁移 |
| 现有持久化 | notesnap:{id} 纯快照全量覆盖（无 log）——本卡补混合存储 |

## 三、混合存储落码（write_path.rs，最小补丁）

| 组件 | 实现 |
|---|---|
| **写路径** | `persist_doc` 增量化：export_update_since(旧快照 vv) → **update log 追加**（KV）；log 达 `UPDATE_LOG_COMPACT_THRESHOLD=500` → **全量快照回写+log 清空**（卡面 compaction 语义；写放大消除——快照低频+增量高频） |
| **打开链** | `load_or_init_doc`：notesnap 快照灌入 → **update log 逐条重放**（seq 升序）→ 最新态；**含无快照分支重放**（首 persist 初始化增量回放——修复过程实锤的漏重放问题） |
| **38b 地基** | log 内 update v1 二进制 = 可编码可合并的传输单元（本卡不传输） |

## 四、DoD 验证（dk40a_tests 3/3 绿）

| DoD | 测试 | 断言 |
|---|---|---|
| **1 零丢失**（用户裁决核心） | `dk40a_concurrent_edits_zero_loss` | 基线 blk-base → fork 双端：A 插 blk-a+追加 "A-contrib"、B 删 blk-base+头部插 "B-contrib" → 增量互换合并 → **blk-a 保留（A 插）/blk-base 删除（B 删，tombstone 胜）/A-contrib+B-contrib 字符均在（字符级）**——块级+字符级零丢失 ✅ |
| **2 重放等价** | `dk40a_log_replay_and_compaction` | 首初始化增量+3 轮编辑（log=4）→ 全新打开重放 == 连续编辑态 ✅ |
| **3 compaction 不变** | 同上 | compact → log 清空（0 条）→ 重放态 == 压缩前 ✅ |
| **4 存量首开** | `dk40a_legacy_first_open_initializes` | 无 notesnap → 空文档+seed → persist → 二次打开 seed ✅（零迁移脚本） |

## 五、四门 + 领地

- TEST=0 CLIPPY=0 FMT=0 DESK=0（DESK 本地 gdk 缺失以 CI 为准——老规矩）；clippy --all-targets 必跑（unused 门）
- 领地：write_path.rs（persist/load 混合存储化+log helpers+测试 mod）——**loro/NoteDoc 本体零改动**（只调既有 export/import/vv API）；IrohTransport/38 面零触碰

## 六、挂账/注记

1. 历史事件无时间戳→重建 stamp 近似（DK-28 同源结论）；loro oplog 天然含时刻——如需精确完成/编辑时刻，38b 可从 oplog 解析（本卡未做）
2. compaction 阈值 500 为常量（非配置面）——38b 实测后如需调节再暴露
3. update log KV 键空间形态——38b 如需 SQL 聚合查询再迁表（rusqlite 现实已声明）
