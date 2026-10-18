# bravo-note：2.8v2/2.9v2 侦察卡回执——澄清互等+批复速览

> 回单方: Bravo ｜ 2026-10-18 17:50 ｜ 对象: alpha-note-bravo催单.md

## 澄清（关键）

两张 bravo-request-*（2.8v2/2.9v2）为 **Alpha 侧预写的侦察报告**（14:38/15:47 pull 到）——卡面已含"现状盘点+建议方案+风险预写死"，**侦察工作已完成**，卡内明注"批复后直接落实现（S2/1.3 窗口切片先例）"。

**Bravo 视角在等 `alpha-approval` 批复卡**——非侦察执行在途。1.3/3.1v2 先例：侦察报告落盘（本例为 Alpha 直落）→alpha-approval 批复→Bravo 按卡直落。无欠账无空转——是流程角色快慢差，回卡即消。

## 批复速览（帮 30 秒裁定）

### 2.8v2 iroh 直传（S ~150 行）
| 待裁定点 | 侦察建议 |
|---|---|
| 通道 | IrohTransport 新增 `send_share(peer, SharePack)`——**需核实 raw bytes 收发原语**（专 doc 单块快照 vs 消息面——我落地时先读 1392 行 transport 实测，卡住即回） |
| 对端收件 | `share:inbox:{ts}` KV 域+cmd 延迟拉取（**不做后台监听**——降复杂度） |
| 门禁 | 发送走 sync_gate.evaluate()（DK-51 先例） |
| 领地 | aurora-sync 跨 crate——**签名预批请求**（send_share 以现有类型面为准） |

### 2.9v2 Web Clipper（M——建议独立派单日）
| 待裁定点 | 侦察建议 |
|---|---|
| 方案 | **A localhost bridge**（B 安装门槛高/C 体验差） |
| 安全线 | token 鉴权（desktop 生成+UI 复制一次）+**只写不读**（零读取面） |
| 领地 | desktop 新 capture_server.rs+扩展仓 `apps/browser-extension/` |
| 节奏 | M 量级——可拆两卡（desktop 桥+扩展）或整卡独立日 |

## 承诺

批复卡落盘 → 下一次 pull 即落。两卡均无侦察卡点——**不存在"先回卡说明卡点"情形**。
