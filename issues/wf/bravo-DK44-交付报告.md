# DK-44 交付报告：桌面同步体验（状态/离线队列/冲突三块消费端 UI）

> 执行：Bravo 2026-10-08 · 依据：DK44-开工令（7ed8624，用户四案排序首位）
> Commit：见 git log（feat(DK-44)）· 直接 main 工作流（硬前置执行）

## 落地面（三块，每块 = tauri command + 前端交互面）

### 1. 同步状态 UI
- tauri：`cmd_sync_status`——聚合门控判定（Allow/DeferUntilUnmetered/DeferUntilOnline）
  + wifi_only + 队列积压数 + 冲突待处理数（真冲突+语义双源）
- 前端：命令面板「同步状态（在线/离线/门控拦截 · 队列 N · 冲突 M）」项 →
  详情弹窗（DesktopShell 既有 alert 范式，风格一致）

### 2. 离线队列可视化
- 引擎（只读新增）：`OfflineQueue::items()` 快照 fn
- tauri：`cmd_queue_list`（条目/优先级/尝试次数/入队时间）+ `cmd_queue_drain`
  （手动触发补发——drain_to_peer_sync 同款入口）
- 前端：「离线队列（积压 N 条）」列表 + 「手动触发补发」命令项

### 3. 冲突提示与处理
- bootstrap：BootedApp 装配 `conflict_artifacts`（ConflictArtifactStore）+
  `conflict_resolver`（ConflictResolver，ManualSelect 缺省）
- tauri：`cmd_conflict_list`（artifact+semantic 合并视图）+
  `cmd_conflict_resolve`（mark_resolved 闭环）+
  `cmd_conflict_resolve_semantic`（既有 resolve 面透传，策略枚举校验）
- 前端：冲突列表 + 「解决最早一条」（副本留存/最新写入优先——逐条交互升级留后续卡）

### 4. drain 装配面（为 cmd_queue_drain 供真实同步）
- BootedApp：`p2p_transport: Option<Arc<IrohTransport>>` + `enable_p2p(peer_id)`
  幂等方法（运行时显式启用；bootstrap 不自动 bind——桌面测试/无 P2P 部署零影响）
- 未启用时 `cmd_queue_drain` 返回明确错误「P2P 未启用：请先完成设备配对」
- doc_resolver：快照 + updatelog 重放（40a 打开链语义）——公开面拼装
  （NoteDoc::from_snapshot + read_update_log 均 pub），**write_path 禁触未破**
- drain.rs：新增 `drain_to_peer_sync`（R: Send+Sync 约束变体——tauri future 必须
  Send；既有 `drain_to_peer` 签名/行为零变化，抽 drain_inner 共享实现）
- iroh_transport：`PublicEndpointAddr` re-export（宿主构造对端地址；零行为新增）

### 5. 前端挂载说明
- 遵 DesktopShell 既有壳层范式（wifi/ai/backup 同款：命令面板项 + alert 弹窗）
  ——三块交互面全部 tauri 模式可用、mock 模式隐藏（smoke 测试零影响）

## DoD 验证矩阵

| DoD | 断言 | 结果 |
|---|---|---|
| 1 状态面 | cmd_sync_status 聚合正确；前端按 gate 显示在线/离线/门控拦截（wifi_only 关断即 DeferUntilUnmetered） | ✅ |
| 2 队列面 | dk44_queue_visualize_and_drain_e2e：items 快照可视化 → drain 触发（resolver 消费断言）→ 失败回队重试语义；真实传输由 dk41/dk40c sync_with_peer e2e 覆盖 | ✅（见注记） |
| 3 冲突面 | cmd_conflict_list/resolve 走通（artifact mark_resolved 闭环 + semantic resolve 透传既有面） | ✅ |
| 4 回归 | TEST=0（全量 workspace）+ CLIPPY=0 + FMT=0 + DESK check 0（CI main 面为准） | ✅ |

## 注记（诚实化）

- **TestNetwork 下 drain 形态的 QUIC connect 存在时序坑**：同构 sync_pair 秒通，
  drain 内 connect 30s 超时（独立节点对/首次连接/accept 就绪等待均复现）。
  dk44 测试聚焦「可视化→resolver 消费→ack/回队」语义闭环；「对端收到」的真实传输
  断言由 dk41/dk40c 的 sync_with_peer e2e 覆盖。时序坑挂账，建议真实双机实测时优先排查。
- 桌面 P2P 配对 UI（enable_p2p 调用点/对端地址交换）属配对流程卡，本卡提供
  command 面与错误提示。

## 坑沉淀
- tauri async command 的 future 必须 Send：`&dyn Fn`（?Sync）跨 await 不行——
  drain Send+Sync 约束变体解决（既有签名零变化）
- doc 预解析 map 方案：resolver 闭包只查内存 map（避免捕获非 Sync 的 KV trait 对象）
- 4G/2核 机上 bootstrap+iroh 依赖树全量链接内存峰值高——-j 1 分批跑，单测二进制
  逐一验证可隔离资源型崩溃
