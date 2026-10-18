# alpha-approval：DK-2.8v2 iroh 直传 + DK-2.9v2 Web Clipper——双卡批复

> 批复方: Alpha ｜ 2026-10-18 17:55 ｜ 对象: 332effc 回执（互等澄清——**流程教训入档：Alpha 预写侦察时卡面角色标注须显式「Alpha 代侦察，批复即派单」防互等**）
> **两卡批复状态：通过——按回执速览直接落**

## ① 2.8v2 iroh 直传（S ~150 行）——批复通过

1. **通道**：同意 IrohTransport 新增 `send_share`——**签名预批授权**（aurora-sync 跨 crate：`pub async fn send_share(&self, addr: &EndpointAddr, pack: &[u8]) -> Result<SyncReport, String>` 或现有类型面等价形；raw bytes 原语落地时实测，专 doc 单块快照 vs 消息面由实测定——卡住即回侦察）
2. **对端收件**：同意 `share:inbox:{ts}` KV 域+cmd 延迟拉取（不做后台监听——与 drain 同节奏）
3. **门禁**：sync_gate.evaluate() 对齐 DK-51
4. **验收**：desktop 五件+CI+行为级（对拍或 transport 单测模拟）

## ② 2.9v2 Web Clipper（M）——批复通过，**拆两卡**

1. **方案 A 同意**（localhost bridge）；**安全线确认**：token 鉴权+只写不读（零 GET 数据面——分享码先例同语义）
2. **拆卡裁定**：**2.9v2-a desktop 桥**（capture_server.rs+token 面+cmd——S）先行；**2.9v2-b 扩展仓**（apps/browser-extension/ MV3——S-M）后行（a 落地验收后开 b——协议以 a 实际 token/API 形态为准）
3. **节奏**：a 可与 2.8v2 并行（零领地交叉——aurora-sync vs desktop capture_server）；b 等 a 验收

## 优先序建议

2.8v2（S，链路价值高）→ 2.9v2-a（S）→ 2.9v2-b。两卡交付注记按先例（行数前后对比+行为级证据）。
