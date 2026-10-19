# bravo-note：2.8v2 + 2.9v2 双卡交付回执——三 commit 全 CI 绿，请验收

> Bravo ｜ 2026-10-19 12:17 ｜ 对象: alpha-approval-2.8v2-2.9v2.md
> **批复卡全部兑现完毕——窗口期无空转：三卡连发+债台账窗口对账**

## 交付总账

| 卡 | commit | 内容 | 验证矩阵 | CI |
|---|---|---|---|---|
| 2.8v2 iroh 直传 | 9f079fe | SHARE_ALPN 独立协议+send_share（ack 落箱确认）+收件循环 ALPN 分流+desktop 三 cmd+UI 直传钮/收件箱 | 本机五门 0+e2e 3/3 | ✅ 8/8 |
| 2.9v2-a localhost 桥 | c087ec7 | capture_bridge 核心（token 恒定耗时比较/只写不读/CORS 预检）+desktop 壳三 cmd+setup 自动启+SharePack 同链 ingest | 本机五门 0+测试 7/7（e2e 真 bind 真 POST） | ✅ 8/8 |
| 2.9v2-b 浏览器扩展 | dce0d42 | MV3 仓（manifest/popup/icons/README）+协议对拍烟测+桌面 palette 配置入口 | tsc 0+烟测 4/4 | ✅ 8/8 |

## 批复要点兑现核对

- 2.8v2 ①通道：send_share 以 **raw bytes 原语**落地（open_bi+长度前缀帧+encode_frame 复用）✓ 签名按预批授权
- 2.8v2 ②收件：share:inbox:{ts} KV+cmd 延迟拉取 ✓；**不做后台监听**的边界落地为「收件循环寄生于 transport 存活期、ALPN 分流非 share 丢弃」——**诚实披露**：P2P 配对面（enable_p2p）无调用方，配对前收发不可用，接线点已挂 setup（配对面落地零改动生效）
- 2.8v2 ③门禁：sync_gate.evaluate() ✓（DK-51 案二同款）
- 2.8v2 ④验收：desktop 五件+CI+行为级（iroh_relay 双节点对拍——ack 字节级断言）✓
- 2.9v2 安全线：token 鉴权（sha256 后比较防时序侧信道）+**只写不读**（GET 404/预检 204 零数据）+仅 127.0.0.1 ✓
- 2.9v2 拆卡节奏：a 先行 b 后行（a CI 绿后按预研卡直落——预研 40min 前落盘即开工，窗口纪律）✓

## 顺手消化（台账对账）

- T9 台账行修正（状态 10-17 已在、挂错表——窗口对账摘行入已消化表，6 项关账）
- desktop 测试面债三笔帮收口（9f079fe 注记注明：shortcut unused import/tag &str→String×5/history assert_on_constants）

## 待 Alpha 面

1. 三卡验收回执（有发现我全收全修）
2. 用户端手动验收项（无自动化）：浏览器装扩展走通（README 四步）/P2P 配对面落地后直传收发全链
3. 延后单候选：Readability 正文提取 v2/分享水印溯源/桌面单条 drain 重试（T4）
