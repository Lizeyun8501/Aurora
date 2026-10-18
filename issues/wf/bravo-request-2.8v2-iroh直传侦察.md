# bravo-request：DK-2.8 v2 iroh 直传分享——接入点侦察（窗口切片）

> 派单方: Alpha ｜ 2026-10-18 ｜ 依据: 5ac516a v1 分享码全链（文本传递语义）+ 审阅定义「iroh 可支撑只读分享链接」
> 性质: **侦察报告非派单**——批复后直接落实现（S2/1.3 窗口切片先例）

## 现状盘点（本机实测）

- **v1 已备**：cmd_create_share（SharePack JSON→b64 码）+cmd_import_share（write_path 全纪律建笔记）——**编码协议可复用**（iroh 通道传同一 SharePack）
- **iroh 底座**：IrohTransport（aurora-sync 1392 行）——sync_with_peer(peer, doc) 点对点双向 CRDT 交换+accept_sync 监听面；drain 模式全链在案（P2P 配对+`sync.peer.addr` KV+sync_gate 门禁）
- **缺口**：对端无「分享收件」监听面；sync_with_peer 双向合并——发布需**单向推送**（只发不收）

## 建议方案（待批复裁定）

1. **通道**：复用 IrohTransport 新增 `send_share(peer, SharePack)`（专用 doc：`share:out:{ts}` 单块快照——或直接 bytes 消息面——侦察 transport 有无 raw bytes 收发原语）
2. **对端收件**：desktop 常驻 accept 面（建议：cmd 延迟拉取收件箱——`share:inbox:{ts}` KV 域+cmd_list_inbox——**不做后台监听**（降复杂度：发布方推送时对端需在线，收件下拉取——与 drain 同节奏））
3. **只读语义**：收件即副本（v1 断链语义延续——对端改动不回传）
4. **门禁**：发送走 sync_gate.evaluate()（wifi_only 对齐——DK-51 先例）

## 风险与边界（预写死）

1. **领地**：aurora-sync（transport 扩展=跨 crate——Alpha 域**签名预批**：`pub async fn send_share(&self, addr: &EndpointAddr, pack: &[u8]) -> Result<SyncReport, String>` 或等价形——Bravo 执行中以编译通过为准卡住即回侦察）
2. **desktop 域**（Bravo 现役）：share_commands 扩展（cmd_send_share/cmd_list_inbox/cmd_import_inbox）+UI（分享行「直传给配对设备」+palette「收件箱」）
3. **验收**：本机五件+tsc/vite+CI；行为级=两实例对拍（单机双 data_dir——drain 对拍先例）或 transport 单测模拟

## 预估

S（~150 行——transport 扩展薄+命令面复用 v1 包格式）；UI ~60 行。
