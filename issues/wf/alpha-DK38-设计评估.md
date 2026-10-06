# DK-38 设计评估：WebSocket 传输层（同步通道）

> 撰写：Alpha 2026-10-06 · 性质：**评估卡（先评估后立项，不写代码）**
> 依据：用户裁决（2026-10-05）——DK-38 下迭代再评；本评估产出立项建议与拆卡方案

## 1. 目标重述

多设备同步根基面：在既有 aurora-sync（增量同步+zstd 压缩，DK-36 资产）之上打通**实时传输通道**——WebSocket 双向同步。

## 2. 三面评估

### 2.1 握手协议（建连后第一段）

- **认证**：device_id + token；token 存储复用 DK-37 secret store sealed 语义（`sync.token.sealed`）——**资产复用明确**
- **版本/能力协商**：hello 帧（proto_version + 能力位：增量压缩/块级合并/游标语义）——不匹配降级或拒连
- **TLS**：rustls（与 DK-31 wasmtime TLS 补洞同源依赖面统一，零新 TLS 栈）
- 评估结论：**低风险**。全部有既有资产承接，无新技术面。

### 2.2 冲突合并（核心难点）

| 方案 | 粒度 | 复杂度 | 与 Aurora 数据模型契合 |
|---|---|---|---|
| LWW（时间戳最后写胜） | note 级 | 低 | 高——现有 updated_at 面直接可用 |
| 版本向量 | note 级 | 中 | 中——需引入 per-note vv |
| CRDT（RGA/Yjs 类） | 块/字符级 | 高 | 低——需重构编辑模型，超纲 |

- **评估结论（2026-10-06 09:05 用户裁决修订：不接受 LWW 语义收缩）**：~~V1 用 note 级 LWW~~ → **CRDT 路线前置**——见 §2.2R 修订。
- 修订后选型：**yrs（y-crdt Rust 官方实现）承载块序列+块内文本，字符级并发无损**；yrs update 二进制天然增量，与 DK-36 zstd 传输压缩互补（update 语义管增量、zstd 管压缩）。
- 依赖新增：`yrs` crate（供应链面：引入时过 cargo audit，版本锁最小次版本）。
- 备选注记：automerge-rs（document 重、change 堆积压缩历史问题）；自研 RGA 否决（CRDT 并发语义测试成本 ≫ 收益——myers 16 处教训）；**回退决策点**：若 DK-40a 实施 yrs 数据模型迁移成本爆炸，回退"块级 RGA+块内 LWW"（丢增量粒度缩到单块内文本）——需再过用户裁决。

### 2.2R 修订：CRDT 集成方案（DK-40a 前置卡）

- **per-note YDoc**：隔离性好、doc 小、增量同步天然 per-note 粒度
- YDoc 结构：YArray（块序列，块级并发无损：双端新增块都保留/删除块都删）+ 每块 YText（块内文本字符级无损）
- **混合存储**（sea-orm 面适配）：现有快照（serde）+ 新增 yrs update log（BLOB 列或新表——Bravo 探明 migration 面后定）——打开时 load 快照+重放 log；快照定期 compaction
- 元数据（title/tags/updated_at）：并发冲突极低，保留 LWW（YMap 可选）——**不做过度设计**
- 版本历史兼容：yrs 任意版本快照（encodeStateAsUpdate(v)）——既有版本历史面可重建
- 核心验收断言（用户裁决对应）：**双端并发编辑同 note——合并后两侧增量全保留（含块级+字符级），零丢失**

### 2.3 断线重连（可靠性面）

- **游标续传**：server 侧单调 seq，客户端持久化 last_acked_seq——重连后增量补发（DK-36 增量+zstd 直接复用）
- **幂等去重**：op_id 去重表（已应用 op 缓存 LRU）
- **心跳/半开检测**：ping/pong 30s 间隔，3 次超时断开重连（指数退避 1s→30s 封顶）
- 评估结论：**中风险**。逻辑直白但状态机分支多（重连中恰逢新写入/双端同时重连），需状态机单测覆盖。

## 3. 依赖与边界

- 传输抽象：**transport trait seam**（DI 资产惯例）——WebSocket 实现与既有 mock transport 并存，测试不依赖真网络
- 运行时依赖：tokio-tungstenite（rustls feature）——新增编译面一个 crate，可接受
- **边界外**：端到端加密（通道 TLS 足够 V1）、块级 CRDT、离线优先全量重建、中继服务端实现（本卡只做客户端协议面+本地 echo 服务端测试桩）

## 4. 拆卡建议（2026-10-06 裁决修订版）

| 卡 | 内容 | 规模 |
|---|---|---|
| **DK-40a（前置）** | aurora-crdt crate：yrs 集成+per-note YDoc 映射+混合存储+双端并发无损断言 | 大 |
| DK-38a | transport trait + WebSocket 连接/握手/心跳/重连状态机 | 大 |
| DK-38b | 同步协议帧（hello/op/ack/seq）+ yrs update 交换 + op_id 幂等 | 大 |
| DK-38c | 游标续传 + 增量补发 + e2e（echo 桩） | 中 |

依赖顺序：DK-40a → DK-38a（可并行）→ 38b（依赖 40a+38a）→ 38c。规模：三 大卡+一 中卡（原评估的规模翻倍判定成立）。

## 5. 结论（裁决后修订）

**已立项（2026-10-06 09:05 用户裁决：不接受语义收缩 → CRDT 前置）**。DK-40a 先行（yrs 集成），38a 可并行开工，38b/c 依前置落地再派。~~悬而未决~~ 已决。

## 6. 边界外更正

原 §3 "块级 CRDT 边界外"表述随裁决作废——CRDT 已是主路径。现行边界外仅剩：端到端加密（V1 通道 TLS 足够）、离线优先全量重建、中继服务端实现（客户端协议面+echo 测试桩）。
