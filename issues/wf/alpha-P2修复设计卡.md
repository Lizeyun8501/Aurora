# Alpha · P2 修复设计卡 + 分支治理清单 — 流水线空闲期技术储备

**撰写**: Alpha 2026-10-07 15:42｜**状态**: ~~设计稿，待 TA 裁决派发~~ → **已实施（6342226，Alpha 代工方案 A）**——依据用户「等待期间开展独立任务」授权+Alpha 代工先例（DK-39/40b 前置段）
**来源**: DK-40a 验收 P2 注记（alpha-DK40a-验收复核.md §二）——38b 前必修挂账

---

## 一、P2 缺陷回顾：log 内容重复膨胀（O(n²)）

**根因**：`write_path.rs::persist_doc` 的 `base_vv` 恒取**旧快照态**（快照仅 compaction 回写，非每轮更新）→ 每轮 `export_update_since(base_vv)` 重导出此前各轮已 append 的 delta → log 条目 bytes **O(n²) 累积**（条数 O(n)，每条内容重复前序）。

**现状影响**（iroh 主线下重新评估）：
- dk40c 主线同步走 LoroDoc 直 `sync_with_peer`——**不经 updatelog KV 面** → 传输放大不成立
- 实际影响收缩为：本地 KV 空间浪费 + compaction 500 阈值提前触发（等于变相频繁全量快照回写）+ 未来 38b 若消费 log 面则放大重现
- **严重级下调：P2 → P3（低优）**，但仍建议修（写路径卫生）

## 二、修复方案对比

| 方案 | 机制 | 代价 | 评价 |
|---|---|---|---|
| **A. 水位键（推荐）** | 新增 `updatelog:{id}:vv` 水位键：append 前 `base_vv=水位 vv`，append 后写回本轮末态 vv | persist 路径 +1 KV 读写（O(1)）；compaction 清 log 时同删水位键 | 精确单轮增量；**水位 vv 与 38b 增量同步天然同构**——一鱼两吃 |
| B. 重放推导 | append 前 `load_or_init_doc` 取 prev 态 vv（与 load 同源） | 每次 persist 多一次全量 log 重放（O(log) 扫描+loro apply） | 零新键但写路径变重；log 大时反而放大 |
| C. 不修 | iroh 主线不消费 log | 零 | 本地空间/卫生债持续累积；38b 复活时踩回 |

**推荐 A**。测试面：断言 3 轮编辑后 log 末条 bytes ≈ 单轮增量（≤ 首条×1.5），条目内容无重复（重放幂等性已有 DoD2 覆盖，回归保底）。

**工作量**：S（~30 行生产码+1 测试）；涉及面：write_path.rs（persist/compact 两处）——**触碰 40a 已验收生产码，需正式开工令**。

## 三、分支治理清单（远端，仅报告不动手）

| 分支 | 状态 | 建议 |
|---|---|---|
| origin/wf/bravo-import-attach | 已全并入 main | **可删** |
| origin/wf/bravo-syncgate-wiring | 已全并入 main | **可删** |
| origin/dev | 实体已 cherry-pick 落 main（5c75d99→9ce1d07、f7a1146→cbd4a42）；唯 8cbc4ab DK-39 重做差异面（SandboxLimits 可配）未带入 | **Bravo 下卡 pull main 后废弃重建**（TA 确认 SandboxLimits 候选去留后删）|

删除动作属破坏性操作，等 TA/Bravo 确认后执行。

## 四、TA 裁决清单更新（累计）

1. 38a/b/c（WS 线）缓派 —— DK-40c 实测支持
2. 40b 收口确认（段 2/3 done + 段 4 dk40c 覆盖，段 1 relay 是否独立成卡）
3. DK-39 SandboxLimits 可配面移植（候选）
4. **P2→P3 降级确认 + 方案 A 派发与否**（本卡）
5. 远端分支清理（两 wf 老分支 + dev 废弃时点）
