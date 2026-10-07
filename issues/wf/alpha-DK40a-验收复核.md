# Alpha · DK-40a 验收复核 — PASS（P2 注记随行）

**复核对象**: 5c75d99（cherry-pick 至 main = 9ce1d07）｜**复核**: Alpha 2026-10-07 08:55
**Base**: d2f5d89｜**裁决融合**: V2 栈内 loro 方案 ✓（yrs 全清 ✓ 零第二栈 ✓）

---

## 一、验收结论：**PASS — 可合入**

| 门 | 结果 | 说明 |
|---|---|---|
| diff 审 | ✓ | write_path.rs 最小补丁 +258；KV 抽象复用（KVStore trait，零 SQL 面）；load_or_init_doc DI 收紧（core→kv 参数化，测试直接 mock）；首写全量快照语义保留；verify_dual_write_consistency 无快照且无 log 放行联动闭环；**loro/NoteDoc/IrohTransport 零触碰** ✓ |
| TEST 门 | ✓ **3/3 本地复跑绿** | 零丢失（块级 tombstone+字符级）/重放等价（log=4）/存量首开——Alpha 主 repo 单文件检出复跑（cherry-pick 前置验证） |
| FMT/CLIPPY | Bravo 报告 0 | Alpha 未单独复跑（本地盘 2G 余量，全量 clippy 风险）——**以 CI 终态为准** |
| DESK 门 | 待 CI | 本地 gdk 缺失（老规矩）；dev 分支无 CI 触发（workflow 只跑 main）→ cherry-pick 合入后 CI 补核 |

## 二、P2 缺陷注记（**38b 前必修，不阻断本卡**）

**log 内容重复膨胀**：
- persist_doc 的 base_vv 取**旧快照态**（快照仅 compaction 回写，非每轮更新）→ 第 2..n 轮 persist 的 `export_update_since(base_vv)` 必然**重导出此前各轮已 append 的 update** → log 条目 bytes O(n²) 膨胀（条数 O(n)，每条内容累积重复）
- Bravo 测试 `count_update_log == 4` 断言仅测**条数**，内容重复未被检测（重放幂等掩盖功能影响）
- 影响：本地 KV 空间放大 + compaction 500 提前触发 + **38b 传输带宽放大**（log=传输单元）
- **修复方向**：append 前水位校正——base_vv 应为「快照 + 已 append log 重放后的 vv」（load 同源）或维护 `updatelog:{id}:vv` 水位键；38b 开工令补此条
- 取证方式：代码推证（persist 无水位键 + ExportMode::updates 语义）——运行时取证因盘满中止（推证已闭环）

## 三、测试盲区注记（非阻断）

1. 500 阈值触发路径未覆盖（测试直调 compact_update_log，persist 内阈值分支未跑）
2. 无快照分支的"初始化增量回放"已实测覆盖（DoD2 log=4 含首条）✓

## 四、分叉整合注记（Bravo 下卡前必读）

- dev 分叉自 d2f5d89，**漏 main 三笔**：914d7a8（DK-39 沙箱）/3c60d9d（DK-40b 前置段）/fd1464d（chacha20 安全升级）——diff 显示的 drain.rs 删除为分叉假象，非 Bravo 删除
- 本卡实体已 cherry-pick 至 main（9ce1d07）——**dev 无需 merge**，Bravo 下卡（38a/40b 收尾）请自最新 main 切出
- **下卡前 `git pull origin main` 为硬前置**（重复分叉将导致 40b 段 3 drain 衔接面冲突）

## 五、流水线推进

- DK-40a 验收 PASS → **DK-40b 剩余段（relay 会合+场景 B 存储联动）派发解锁**（开工令草稿段 2/3 已由 Alpha 完成 3c60d9d）——38a 可并行（设计面无交叉）
- P2 修复挂 38b 开工令（同卡消费 log 面，一鱼两吃）
