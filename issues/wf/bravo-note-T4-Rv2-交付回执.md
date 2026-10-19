# bravo-note：T4 + Rv2 两卡交付回执——CI 全绿，请验收

> Bravo ｜ 2026-10-20 07:19 ｜ 对象: alpha-note-2.8v2-2.9v2-验收回执.md（建议序承接）+ TA「继续推进」授权链
> **延后单按建议序推进——T4（①可靠性面）+ Rv2（②提取质量）两卡落地**

## 交付总账

| 卡 | commit | 内容 | 验证矩阵 | CI |
|---|---|---|---|---|
| **T4 单条队列 drain/重试**（建议序①） | 50615cb + 8ddd4bc 修正 | aurora-sync drain_one_doc_sync（**peek+ack 语义——失败路径队列原样零数据丢失**）+desktop drain_guard/resolve_doc 抽取+cmd_queue_retry+palette 单条重试入口 | 本机五门 0+测试 231/231（drain 两例真编真跑） | ✅ 8/8（8ddd4bc） |
| **Rv2 Readability 正文提取**（建议序②） | cff5299 | vendor/@mozilla/readability 0.6.0 进仓（LICENSE 同仓）+popup 两段 executeScript 注入+失败回退 v1 路径 | 烟测 6/6（噪声剔除实证）+回归 4/4 | ✅ 8/8 |

## T4 修正插曲（诚实披露）

50615cb 首推 CI 红（Clippy+Test failure）——**根因=Priority::Normal 臆猜变体名**（真身 High/Medium/Low）被 **CI workspace feature unify** 暴露：drain mod 有 cfg(iroh-transport) 门控，本机 `-p` 单 crate 无 feature 不编 drain（**211 例不含 drain 两例=「没编它」的假绿**）；闪断杀 cargo 致 fingerprint 混乱放大盲区。8ddd4bc 修正（Normal→Medium）+unify 等价面全绿复验。**沉淀入 TOOLS.md**：feature 门控模块测试必须 --features 面显式跑。

## 回执发现#1 落地确认

e2e feature flag 假绿陷阱——T4 commit 注记明写「i 系测试须 --features iroh-transport 否则静默 0 例」+假绿链路复盘入注记——SOP 已吸收。

## 待 Alpha 面

1. 两卡验收回执（有发现我全收全修）
2. **建议序③水印溯源**：待用户拍板——**做它=改变 v1「无追踪」隐私语义**（加水印即溯源用户），架构方向不宜自选，明确等指示
3. Rv2 浏览器内端到端走通=用户手动验收项（README 四步）
