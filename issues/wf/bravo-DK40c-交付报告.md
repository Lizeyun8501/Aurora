# DK-40c 交付报告：iroh P2P 双端 e2e 实测（含离线补发）+ NoteDoc 衔接

> 执行：Bravo 2026-10-07 · 依据：alpha-DK38-设计评估 21ea11f 修订——Alpha 建议选项 A 先行：
> "40a 落地后实测 iroh 双端 e2e（含离线补发）再定"，为用户裁决 A/B 提供数据输入
> Commit：见 git log（feat(DK-40c)）

## 背景

- 21ea11f 裁决：yrs 作废（栈内 loro 已承载 CRDT），40a 缩为混合存储（已闭环 5c75d99）
- DK-38 WebSocket 三卡缓派，悬决：**A: iroh P2P 先行** vs **B: 坚持 WebSocket C/S**
- IrohTransport 693 行既有资产（DK-08：内存仿真网双向收敛/星型/环形多跳已覆盖）

## 本卡实测内容

### 1. dk40c_p2p_offline_catchup_converge（离线补发——裁决核心场景）
- 双端各自**离线多轮编辑**（互不连）→ 回线**单轮 sync** → 版本向量交换
- **断言**：双向收敛一致，互含对方离线期全部编辑（[A-offline]/[A-more]/[B-offline]/[B-more] 全量到达）
- **结论**：iroh P2P 离线补发语义 = CRDT 版本向量天然能力，一次同步补全，无需额外队列协议

### 2. dk40c_notedoc_over_p2p（40a 数据面衔接）
- NoteDoc（40a 混合存储数据面）底层 LoroDoc **直接经既有 sync_with_peer 通道传输**
- **断言**：对端 from_doc 恢复后**正文逐字一致** + **块树一致**（2 块全保留）；
  反向 B 端 add_block 增量 sync 回 A（3 块收敛）
- **结论**：38b"updatelog 为传输增量单元"可行性得到验证——NoteDoc 块级 CRDT 面与
  iroh 传输通道无缝衔接，无需协议转换层

### 3. 基建：feature 级联
- `aurora-sync/iroh-transport = ["dep:iroh", "loro-crdt", "aurora-core/loro-crdt"]`
  （iroh 传输的就是 loro doc，core 的 NoteDoc 随 feature 自动可用）

## 验证矩阵

| 门 | 结果 |
|---|---|
| dk40c 新增 | 2/2 ✅（离线补发收敛 + NoteDoc 过 P2P 双向） |
| aurora-sync 全量 | 217/217 ✅（含 DK-08 既有 4 场景回归） |
| CLIPPY | 0 ✅（-p aurora-sync --features iroh-transport -D warnings） |
| TEST（全量 workspace） | 0 ✅ |
| FMT | 0 ✅ |
| DESK | CI main-only（dev push 不触发，历史同款），本地门为准 |

## 裁决建议（数据面）

实测支持 **选项 A**：
- 离线补发零额外协议（版本向量天然语义）✅
- NoteDoc/块级数据面与 P2P 通道直连 ✅
- 40a 混合存储（updatelog）→ 38b 增量传输单元路径通畅 ✅
- WebSocket 栈为"未验证缺口"预建的成本可避免；企业中控形态另立远期卡

## 边界外（注记，未做）
- 真实网络（非内存仿真）双机实测——需物理环境，内存仿真语义等价
- iroh relay 中继（一端永久离线场景）——38b 若立项再验
