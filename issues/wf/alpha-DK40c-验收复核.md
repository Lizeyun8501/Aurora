# Alpha · DK-40c 验收复核 — PASS（V1 同步主线实测闭环）

**复核对象**: f7a1146（cherry-pick 至 main = cbd4a42，含合并修复）｜**复核**: Alpha 2026-10-07 15:10
**Base**: 5c75d99（dev 分叉）｜**同批**: 8cbc4ab（DK-39 重做——未带入，见四）

---

## 一、验收结论：**PASS — 可合入**

| 门 | 结果 | 说明 |
|---|---|---|
| diff 审 | ✓ | iroh_transport.rs +134 纯测试增量（dk40c 两测）；Cargo.toml feature 级联一行；**生产代码零改动** ✓ |
| TEST 门 | ✓ **本地复跑 221+5 全绿** | dk40c_p2p_offline_catchup_converge（双端离线多轮编辑→回线单轮 sync 版本向量补发全量，双向收敛）+ dk40c_notedoc_over_p2p（NoteDoc 底层 LoroDoc 直经 sync_with_peer，正文逐字+块树一致+反向块级增量收敛）——**V1 选项 A（iroh P2P 先行）实测证据链闭合** |
| FMT/CLIPPY | ✓ 0/0 | 本地 -D warnings 复核过 |
| 合并修复 | ✓ | cherry-pick 冲突手工合并（write_path.rs 保 main cfg 注记版；iroh_transport.rs 双方新增段拼接+补 dk40b 收尾 `}`）——**合并后 226 测全绿复核过** |

## 二、feature 级联（Bravo 治本方案）与 Alpha cfg 修复的关系

- Bravo：`iroh-transport = ["dep:iroh", "loro-crdt", "aurora-core/loro-crdt"]`——语义治本（iroh 面天然激活 DK-40a 代码）
- Alpha（8fc41ae）：write_path.rs 两处 cfg 门控——窄面防御（任何不级联的解析面都能编译）
- **两者互补，合并后共存**：级联保证主路径面正确性；cfg 保证极端面（未来新 crate 单独依赖 aurora-core）不复发
- Bravo 独立发现同一问题（8cbc4ab"顺手修 40a 遗留 KVStore import cfg 门控"）——**诊断独立收敛，佐证根因判定正确**

## 三、DK-40c 对流水线的影响（供 TA 裁决）

- Bravo 实测数据支持**选项 A（iroh P2P 先行）**：离线补发场景 B 全链实测通过（dk40c_p2p_offline_catchup_converge 即 40b 段 4 的场景 B 等价覆盖）
- **建议**：38a/b/c（WebSocket 线）缓派——iroh 主线实测已闭环，WS 线降远期；40b 段 1（relay 会合）是否仍需独立成卡待 TA 定（dk40c 已用 iroh 测试网络直连，relay 会合是 NAT 穿透强化面）
- 40b 段 2/3（Alpha 已完成 3c60d9d）+段 4（dk40c 覆盖）→ **40b 可判实质闭环，待 TA 确认**

## 四、DK-39 重做（8cbc4ab）未带入说明

- main 已有 Alpha 版 DK-39（914d7a8，07:59 验收 PASS：4/4 绿+epoch+limiter+可配构造器）
- Bravo 8cbc4ab 为重复劳动（dev 分叉未拉 main——**分叉注记在 56e8ff6 已警示，Bravo 未执行 pull**）
- Bravo 版差异面：Config+SandboxLimits 可配（serde default 向后兼容）+ wasmtime36 limiter message 状态位还原——**若 TA 认为 manifest 可配面有价值，可作为后续卡把 SandboxLimits 移植进 main 版**；本次不带（避免双 DK-39 实现）

## 五、分叉治理（再次警示）

- dev 链：5c75d99 → 8cbc4ab → f7a1146，全部基于 d2f5d89 分叉点，**三度未 pull main**
- 下卡硬前置：Bravo `git pull origin main && git rebase`（或按 TA 指示切新分支）——main 现含 8fc41ae（cfg）+cbd4a42（DK-40c），dev 的 8cbc4ab/f7a1146 已被 cherry-pick 取代，**dev 分支建议废弃重建**
