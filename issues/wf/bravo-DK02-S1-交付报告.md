# Bravo 交付报告 — DK-02 S1 回收站（core 面）

> 基线 571da67 · 领地 write_path.rs + layered.rs（未动——事件选型免追加）+ desktop tauri · 预算 8–10 人日内

## 一、做了什么

### 1. 软删改造 `write_path::delete_note`（签名不变，调用方透明）

- 物理 `note:{id}` / `notesnap:{id}` **保留**（恢复数据源）；
- 写 `trash:{id}` 标记键：`TrashedNote { note_id, deleted_at_ms, title }`——title 快照明文（**Alpha 裁决采纳**：加密笔记 title 在 NoteRecord 层本就明文，列表零解密开销）；
- `NoteDeleted` 事件照发（投影/搜索清理路径不变）；
- **附件级联移出**（回收站期间附件保留，随 purge 收编）；
- 幂等：重复删除仅刷新 deleted_at_ms。

### 2. 事件选型裁决（Bravo，任务书授权）：**重放 `NoteCreated`，不新增 `NoteRestored`**

理由：①投影/搜索索引为任务书禁改冻结面，`NoteRestored` 无消费者、无法恢复可见性——除非改投影（违约）；②`NoteCreated` 重放走完整建条目路径（搜索 `index_note` + blocks 派生重建，DK-01W 先例）；③事件字典 41 冻结面**零改动**（layered.rs 无追加——比派发书预估的追加式更省）；④`open_note_content` 复用：恢复以明文重放，锁定态加密笔记 fail-closed（`Crypto` 错误——绝不以密文进索引）。

### 3. 新原语（write_path.rs）

| 原语 | 语义 |
|---|---|
| `restore_note(ctx, id)` | trash 校验 → 删标记 → NoteCreated 重放；不一致态（trash 在键不在）报 NoteNotFound |
| `purge_note(ctx, id)` | **仅对 trash 中笔记生效**（误 purge 物理上不可能）；物理删三键 + 附件级联 + NoteDeleted 幂等重发 |
| `list_trashed(core)` | scan_prefix("trash:")，删除时间倒序 |
| `purge_expired(ctx, days)` | 过期批次 purge（30 天自动清空调度接线**挂 S2**，本卡只交付原语） |

### 4. desktop tauri 命令（半接入三查：非测试专属）

`cmd_list_trashed / cmd_restore_note / cmd_purge_note`（照 cmd_delete_note L447 owned-ctx 先例）+ invoke_handler 注册。TS 绑定与 UI 归 Alpha 后续切片（未触碰）。

### 5. sync 键域裁决（Bravo，任务书授权）：**trash: 进同步键空间**

理由：多端回收站一致性（A 端删 B 端可见标记）。**诚实化挂起**：跨端仅存储层同步，B 端投影/回收站列表的事件级消费（NoteDeleted 跨端重放）挂 S2 设计。

## 二、验证矩阵（本地四门槛）

| 门 | 结果 |
|---|---|
| `cargo test -p aurora-core -p aurora-bootstrap`（CARGO_INCREMENTAL=0） | ✅ **437 passed / 0 failed**（core 405+doc、bootstrap 5+5+2；新增 dk02_s1_trash 5 例） |
| clippy -D warnings（core+bootstrap --all-targets） | ✅ 0 |
| fmt 逐包（core/bootstrap/desktop/ai --all --check） | ✅ 净 |
| desktop check/clippy | ⚠️ **本地无 GTK dev 头文件（环境约束，gdk-sys build 失败）——沿 DK-10 切片3 先例由 CI desktop-check job 兜底**；fmt 已本地过 |

**行为级断言面**（dk02_s1_trash.rs，环境无关口径）：
1. **主闭环**：delete 后 note:/notesnap: 保留 + trash 可列（title 快照）+ **NoteDeleted 事件落库（投影清理输入实证）**；restore 后 trash 消失 + **NoteCreated 重放落库（投影重建输入实证）** + 内容逐字节 round trip；purge 后三键全消 + 二次 purge 拒绝；
2. **加密 round trip**：set_note_encryption(aes256gcm) → enc1: 密文落库 → delete → restore → 解密逐字节一致（content_cipher DK-07 S3 先例）；
3. **附件**：delete 后 blob 可 get（回收站保留）+ meta 不清；purge 后 meta 空且 blob GC 兜底（内容寻址键仍在）；
4. **purge_expired**：过期清、未过期留；
5. **状态机**：双重 delete 幂等 / purge 后 restore 拒 / 未删笔记 purge 拒（误 purge 不可能）。

**既有测试同步**：isomorphic_write_path 对拍 4 断言从「delete 全清」更新为「软删保留 + trash 两端口径一致 + purge 后全清」（行为语义变更的正确传播）。

## 三、挂起项（诚实化）

1. **搜索投影行为级断言（tantivy 命中/消失）在本测试环境不可靠**：bootstrap source 回调（线程+新 runtime block_on scan）+ 增量 catch_up 链在测试环境下 index 不生效（doc_count=0），**既有问题非本切片引入**（直调 backend.index_note 生效、rebuild 失效——source 回调静默空）。投影驱动改用事件流落库实证（输入正确性），**source 回调独立排障**建议另开小卡；
2. **verify 语义缺口（设计发现）**：搜索投影 verify「index < source → rebuild」在软删后会把 trash 中笔记**索回**（source 扫 note: 不排 trash）——本卡未触碰投影（禁改面），**S2 必须给 source 回调加 trash 过滤**，否则生产重建路径会把回收站笔记泄回搜索；
3. 30 天 purge 调度接线 / 跨端投影消费 / UI 面均挂后续切片（任务书既定）。

## 四、CI

push 后回填（Alpha API 独立确认口径）。

— Bravo 2026-09-28（DK-02 S1）

## 五、CI 终验（含修复轮回填）

- 首轮 f175692 Test job 失败：mobile-ffi list_notes 软删泄漏（任务书预警点）——已修（list_notes 过滤 trash + discard_note 替换 import 失败清理，行为级断言抓到）；
- 修复轮 57c8f95 = **SUCCESS**（五 job 全绿）。S1 闭环。

— Bravo 2026-09-28
