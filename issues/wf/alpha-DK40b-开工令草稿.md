# DK-40b 开工令（草稿，40a 验收后派发）：iroh 同步全链——relay 会合+离线补发衔接+双端 e2e

> 撰写：Alpha 2026-10-06 23:30 · 依据：DK-38 §8 裁决 A（V1 同步主线=iroh P2P）
> **状态更新 2026-10-07 09:00：DK-40a 验收 PASS（56e8ff6）→ 本卡正式解锁派发**
> **Bravo 开工前置（硬性）**：`git pull origin main`——dev 分叉漏 main 三笔（914d7a8/3c60d9d/fd1464d），不 pull 必撞 drain.rs/沙箱/chacha 面
> **范围收缩确认**：段 2（addr 交换）+段 3（drain 衔接）已由 Alpha 完成（3c60d9d，e2e 3/3 绿）——**Bravo 范围=段 1（relay 会合）+段 4（场景 B 存储联动 e2e）**
> **38b 联动补条**：40a 验收 P2 注记（log 内容重复膨胀——base_vv 无水位）在本卡场景 B 消费 log 前必修，或并入 38b 开工令

## 23:30 栈内探明底账（Alpha 实测）

| 面 | 现状 | 缺口 |
|---|---|---|
| iroh Endpoint | `Endpoint::builder(Minimal)`——**无 relay/无发现的纯直连** | 生产 relay 会合未配（nat 穿透兜底缺失） |
| 设备地址交换 | EndpointAddr 手工交换（测试模型），注记"生产经二维码/账户服务带外交换" | **带外交换机制未实现**（V1 建议：手动二维码/剪贴板导出导入，零服务端） |
| offline_queue | SQLite sync_queue+优先级+幂等键+compress_batch——API 完整 | **与 iroh transport 的 drain 衔接层缺失**（无 flush/resend/retry 触发路径） |
| sync_gate | evaluate Defer* 决策 → offline_queue 入队 | 网络恢复→自动 drain 重发未确认（疑似缺） |
| 链路健康 | dk08_p2p 4/4 绿（双向/环/星型，1.24s） | 仅内存网络（Minimal）；真实 UDP/relay 路径未验 |

## 本卡范围

### 1. relay 会合配置

- Endpoint 构造去 Minimal：启用 iroh relay（V1 用公共 relay 或 env 可配自建 relay URL——Bravo 按 iroh 1.x API 现状定，卡内注记选型）
- 断言：双端经 relay 会合完成一次 sync（CI 沙箱真实 UDP 不可用——relay 路径测试策略：本地 relay spawn（iroh 支持 local relay test helper）或注记 CI 边界、以手动实测补充）

### 2. 地址带外交换（V1 手动模型）

- 导出/导入本机 EndpointAddr（serialize→剪贴板/文件/二维码编码由桌面/移动 UI 后续卡消费，本卡只做 API：`export_addr()/import_addr()`）
- 与既有 bootstrap/配对面关系：探明后复用或新增，卡内注记

### 3. offline_queue → iroh drain 衔接

- `SyncGate` 网络恢复事件（provider 状态变化）触发 `drain_to_transport()`：dequeue_batch → compress_batch → iroh sync_with_peer 逐批发送 → 成功后删行（幂等键防重）
- 断言：入队 N 条 → 模拟离线 → 恢复 → 全部送达且对端 NoteDoc 收敛、队列清空

### 4. 双端 e2e（收口）

- 场景 A 在线双向：A/B 各自编辑 → 互同步 → NoteDoc 收敛（复用 dk08 形态+NoteDoc 真容器）
- 场景 B 离线补发：B 离线期间 A 编辑 → B 恢复 → drain → 收敛（**场景 B 是本卡核心新增**）
- 与 40a 衔接：NoteDoc 实例从混合存储载入（40a 交付面），update log 键域对接

## DoD

1. 场景 B e2e 绿（离线→恢复→补发→收敛，幂等不重发）
2. relay 会合路径打通（或 CI 边界注记+本地 relay 测试绿）
3. export/import_addr API+单测
4. drain 衔接单测（网络恢复触发、幂等、压缩批次）
5. 四门 TEST=0 CLIPPY=0 FMT=0 DESK=0

## 边界外

- 二维码/剪贴板 UI（桌面/移动 UI 卡）
- 多设备房间/发现服务（V1 两两配对）
- 服务端 relay 自建部署（env 配置预留即可）
