# Bravo 派发任务书 — DK-02 S1：回收站（core 面）

> 派发对象：Bravo · 生成：2026-09-28 14:35 · 基线 `e8acb3f`（main）
> 优先级：P1（M1 尾巴解锁：DK-04/12/13 依赖链堵点）· 预算：8–10 人日
> 上游：排期计划 v26-排期计划-20260928.md（S1→S2 目录树→S3 智能文件夹）

## 〇、现状实证（Alpha 侦察，派发前已核）

- `write_path.rs::delete_note`（L582）现为**物理删**：`note:{id}` 与 `notesnap:{id}` 两键同删 +
  附件级联（meta 清、blob 留 GC）+ NoteDeleted 事件（投影清理）——注释写「软删除」名不副实；
- AppEvent 枚举（layered.rs）：NoteContentChanged / NoteMetadataChanged / NoteCreated /
  NoteDeleted / SnapshotRequested / SyncCompleted —— **无恢复类事件**；
- 附件在 KV `attachblob:{sha256}`（内容寻址 + GC 兜底，DK-09 先例）。

## 一、S1 范围（core 面）

### 1. 回收站标记键（新）

```
trash:{note_id} → JSON { deleted_at_ms: i64, title: String }
```
- title 快照入标记键（回收站列表零解密开销——加密笔记标题也是密文，列表只显示「（加密笔记）」或
  快照明文，**Alpha 裁决：入快照明文**，回收站为本地面可接受）；
- **delete_note 改造**：物理删 → 写 `trash:{id}` + 保留 note:/notesnap: 键 + NoteDeleted 事件
  照发（投影清理语义不变——主视图/搜索立即消失）+ **附件级联挪出**（回收站期间附件保留，
  恢复即用；彻底删除时才级联）；
- **restore_note(ctx, note_id)**：删 `trash:{id}` + 恢复投影可见性——事件选型 Bravo 裁决
  （NoteCreated 重放 or 新增 NoteRestored 事件；若新增：layered.rs 枚举 + name() + channel()
  三处 + 事件字典同步追加，**追加式改动需在报告列明**）+ blocks 派生重建（DK-01W 先例）；
- **purge_note(ctx, note_id)**：原物理删逻辑收编（两键删 + 附件级联 + NoteDeleted 幂等）；
- **list_trashed()**：`scan_prefix("trash:")` → Vec<TrashedNote { note_id, title, deleted_at_ms }>。

### 2. trash: 键的同步语义（Bravo 裁决+理由入报告）

建议：**进同步键空间**（notesnap 已同步，标记不同步 → 多端回收站视图漂移）；若同步引擎按前缀
白名单过滤则需追加——侦察后定，改动列报告。

### 3. 30 天自动 purge

**S1 挂起**（调度面同 DK-17 S2 先例——boot 扫描或定时器后置）；S1 仅提供 `purge_expired(days)` 供未来调度调用。

## 二、领地与禁改

- **可改**：`crates/aurora-core/src/write_path.rs`（delete_note 改造 + 三新函数）、
  event_bus/layered.rs（可选 NoteRestored 追加）、**桌面 tauri 命令**（cmd_restore_note /
  cmd_purge_note / cmd_list_trashed——照 cmd_delete_note L447 先例）；
- **禁改**：投影/搜索索引内部逻辑（消费既有事件即可）；apps/desktop/src/components/**
  （回收站 UI 是 Alpha 后续切片）；移动端（下片）。

## 三、DoD

1. 单测闭环：写笔记 → delete（主列表/搜索消失实证——投影清理断言）→ trash 可列 →
   restore（重新索引可搜到——round trip 内容一致）→ purge（物理键消失 + 附件级联）；
2. 加密笔记删除/恢复 round trip（content_cipher 先例——解密后逐字节一致）；
3. 附件行为断言：删除后 blob 可查（回收站保留）、purge 后 GC 兜底路径存在；
4. 全量 `cargo test -p aurora-core` 不回退 + clippy 0 + fmt 净（**逐包**：core/bootstrap/desktop）；
5. CI 五 job 绿（Alpha API 独立确认）；交付报告：事件选型理由 / sync 语义裁决 / 挂起项。

## 四、规约与警告

- commit 前缀 `feat(DK-02):`；阻塞写 `bravo-request-DK02S1-<主题>.md`；
- **半接入三查**：restore/purge 不得仅存在于测试——tauri 命令可达；
- 事件字典追加走**追加式**（不动既有 code——41 字典冻结面）；有争议提 Alpha 裁决；
- 注意 `delete_note` 调用方（desktop cmd L480 + mobile-ffi？）——**rg 全调用方**，语义变更
  （物理→标记）对调用方透明（签名不变），但 mobile 侧若依赖「删后 notesnap 消失」需核查。

— Alpha 派发 2026-09-28
