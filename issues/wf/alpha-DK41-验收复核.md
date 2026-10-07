# Alpha · DK-41 验收复核 — PASS（整合后 PASS，Alpha 代工补齐修正 1 + DoD 2/3）

**复核对象**: b8f5f70（cherry-pick 至 main + Alpha 整合 = 893ac03）｜**复核**: Alpha 2026-10-07 23:58
**派发**: DK-41 开工令（7d11a91，用户裁决 A）｜**Bravo 交付**: 22:57（实体 161 行）

---

## 一、结论：**PASS（整合后）——V1 iroh P2P 持久化面闭环**

| 门 | 结果 | 说明 |
|---|---|---|
| 实体 diff 审 | ✓ | UpdateSink 钩子设计干净（None=纯内存行为不变向后兼容）；两 import 点全挂；三原语 pub 化仅可见性调整零逻辑改动 |
| 实体 diff 审扣分项 | ⚠ | 见三——修正 1 缺失（基线无 P2）+DoD2/3 未做+硬前置违规 |
| DoD 1 内存收敛 | ✓ | dk41_updatelog_bridge_persistence（Bravo 原测，sink 改用修正 1 原语后复跑绿）|
| DoD 2 SqliteStorage | ✓ | **Alpha 补** dk41_bridge_sqlite_persistence：文件级双实例+重启（重开连接）+打开链重放恢复对端编辑+水位跨重启持久 |
| DoD 3 水位推进 | ✓ | **Alpha 补** 断言：sink 落 log 后 `updatelogvv:` 双端落地且跨重启持久——膨胀复发路径永久锚定 |
| 四门 | ✓ | sync 223+5 / core 439 / workspace check / clippy+fmt 全零（-D warnings）|

## 二、Alpha 整合增量（893ac03，+136/-12）

1. **修正 1 落码**：`UpdateSink` 签名改 `(update, post-import vv encode)`——两 import 点 import 后 `oplog_vv().encode()` 现成可得；新原语 **`receive_update_log`**（append+水位推进一体，pub）——接收侧免知水位细节，宿主/测试单步调用
2. **DoD 2**：SqliteStorage 文件级双实例 + tempfile dev-dep（aurora-sync/Cargo.toml）
3. **DoD 3**：水位落地断言（内存+SQLite 两测均锚定）
4. 三处编译修：unused import / `Arc` 上 get 需 `as_ref()` + KVStore trait use / tempfile 声明

## 三、⚠ 硬前置违规三连记录（流程升级事项）

- Bravo dev 分叉点仍为 d2f5d89——**第三次未 pull main**（40a/DK-39/DK-41 三度）
- 本次后果实体化：b8f5f70 不含 8fc41ae（cfg）/6342226（P2 水位键）/7d11a91（本卡开工令与修正条款）——**修正 1 未被执行非 Bravo 违令，而是其 git 视野里根本没有开工令**
- 措施：Alpha 代工补齐（本整合）；**dev 分支建议即刻废弃**（Bravo 下一卡必须从 main 重建）；建议 TA 侧沟通 Bravo 的工作流（为何 push 前不 fetch/pull——三度重复）

## 四、V1 同步主线状态（裁决 A 后）

- IrohTransport（QUIC+vv+增量）+ 混合存储（P2 水位键）+ 桥接（sink 持久化钩子+水位对齐）——**持久化面闭环**
- 余项：relay 会合（远期）、真实双机实测（产品化阶段）
- 插件市场前置：SandboxLimits 移植（295a323 评估在案，待 TA 拍板两问）
