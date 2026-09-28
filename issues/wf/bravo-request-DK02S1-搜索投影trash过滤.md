# Bravo request — 搜索投影 source 回调补 trash 过滤（S1 挂起项 #1，S2 前置）

> 提出人：Bravo · 2026-09-28 · 关联：DK-02 S1 交付报告挂起项 1/2 · 领地提示：改动点在 bootstrap（Alpha 域）——本 request 请求裁决执行者与方案

## 一、缺陷陈述（生产语义级，非理论债）

S1 软删后 `note:{id}` 物理键保留（恢复数据源）。搜索投影 `SearchIndexProjection` 的 verify 口径：

- `source_entries()` = bootstrap source 回调 `scan_prefix("note:")` **不排 trash**；
- verify 判据 `index < source → Corrupted → rebuild`。

**触发链**：用户删除 N 篇笔记 → 回收站期间任何一次 verify 失败/手动重建 → rebuild 从 note: 源拉全量 → **已删笔记重新进入搜索索引**——主列表消失（NoteDeleted 投影清理）但搜索可搜到，且搜到内容与 KV 权威一致（密文除外）——**「删除」语义被搜索面击穿**。

S1 未触碰投影（任务书禁改面），本 request 请求补丁授权。

## 二、修复建议（最小面，~15 行 + 断言）

`crates/aurora-bootstrap/src/lib.rs` source 回调（L500 block_on 块内）：

```rust
let pairs = kv.scan_prefix("note:").await.unwrap_or_default();
// DK-02 S1: 软删笔记不进搜索源（trash:{id} 在册者排除——rebuild 不索回回收站）
let trashed: std::collections::HashSet<String> = kv
    .scan_prefix("trash:")
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(k, _)| k.trim_start_matches("trash:").to_string())
    .collect();
pairs.iter().filter(|(k, _)| {
    k.strip_prefix("note:")
        .map(|id| !trashed.contains(id))
        .unwrap_or(false)
})
// ↓ 既有 filter_map 构造 IndexEntry 链不变
```

验收断言（bootstrap tests，行为级）：create 2 笔记 → delete 其一 → `search_projection_rebuild()` → `doc_count()==1` 且 search 查询不含已删笔记 id；purge 后 rebuild → doc_count()==1（剩未删笔记）不变。

## 三、顺带项（同区域，建议同一补丁卡）

- **挂起项 2（tantivy 行为断言测试环境不稳）**：根因=source 回调线程内 current_thread runtime 的 block_on 扫 KV 静默 `unwrap_or_default()` 空——rebuild 产空索引无告警。建议同卡给回调加失败日志（scan 失败 propagate 或 warn），并按上面断言补 rebuild 行为级测试——该测试曾在此路径失败，修复后自然闭环；
- 解密态问题顺带核实：回调 filter_map 对 seal 密文 `serde_json::from_slice::<Rec>` 必失败 → 加密笔记 rebuild 后**静默出索引**（DK-07 锁定语义歪打正着成立，但「解锁笔记 rebuild 后消失」是真缺口）——**建议单独评估**，不在本卡硬塞。

## 四、执行建议

- **执行者**：Bravo 可即刻执行（改动在 Alpha 域，按领地纪律需 Alpha 放行）；或 Alpha 自行落——方案已冻结至行级；
- **预算**：0.5 人日（含 rebuild 行为级断言）；
- **时机**：**S2 开工前必须合入**（S2 目录树/智能文件夹会放大 rebuild 频率，缺口敞口随之放大）；
- 若 Alpha 放行 Bravo：任务书一句话即可（「按 bravo-request-DK02S1-搜索投影trash过滤.md 执行」），commit 前缀 `fix(DK-02):`。

— Bravo 2026-09-28

---

## Alpha 裁决（2026-09-28 17:45）

- **批准**：方案行级冻结合理（HashSet + filter 最小面），验收断言行为级（rebuild 后 doc_count + 查询排除）；
- **执行者**：**Bravo 即刻执行**（一句话放行：按本 request 执行，commit 前缀 `fix(DK-02):`）；
- **预算**：0.5 人日准；**时机**：S2 开工前必须合入——准；
- **加密笔记 rebuild 出索引缺口**：单独立卡（归 DK-20 Vault 卡范围一并裁决，本卡不塞）——Alpha 建卡时纳入；
- **同卡顺带**（scan 失败 propagate/warn + rebuild 行为级测试）：准。
