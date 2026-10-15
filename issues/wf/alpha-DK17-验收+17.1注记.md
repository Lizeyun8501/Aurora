# DK-17 L2 验收 + DK-17.1 修复注记（2026-10-15 19:21 验收轮）

**交付**: 92803bd（Bravo L2）｜ **复核**: Alpha ｜ **结论**: **DK-17 关账**；审计 2 发现（A-6/A-7）随 **DK-17.1 Alpha 直做完成**——单元 **100/100** + e2e **21/21** 全绿（含 17.1 修复与合约 0.3.0 bump）

## 一、L2 独立复核

| DoD 项 | 复核 | 结果 |
|---|---|---|
| share 命中读 | read-back 探针（guest 面）断言 | ✅（本地真跑） |
| 未 share 拒/写拒 | namespace-denied 不变+write-back 探针（share 只读——**write 路径不豁免**实证 guard 1289 行 `event == "read-doc"` 条件） | ✅ |
| accept_share=false 拒 | share 动作本身拒（opt-in fail-closed——设计稿 2.2 语义成立） | ✅ |
| revoke 后拒 | e2e 覆盖（修正注记：revoke 例缺 A init 的 e2e 编排问题已由 Bravo 自纠，断言全走 guest 面探针——host-invoked 特权面无 guard 的语义澄清正确） | ✅ |
| 豁免顺序语义 | **namespace 环内豁免、method 环仍校验**（1312-1320）——share 授予跨空间读而非"读"本身，与 storage_methods 正交；to 匹配单向（1294-1298）；share-read 全量留痕（不走 first-call 节流——特权路径更严格，可辩护） | ✅ |
| A-5 | WIT init 扩参 plugin-id+OnceLock 派生（hello 写死退役） | ✅ |
| A-1 | CallerCtx 接口位（call_host_function_with_ctx+挂载，校验位 TODO——按稿不给实现） | ✅ |
| 15c | acceptShareBadge 中性色接线 | ✅ |
| 全绿 | 本地 wasip2 全 guest 重建后 21/21+99 单元 | ✅ |

## 二、审计发现（均不阻塞关账）

**A-6（低——合约版本纪律）**：init 扩参=world 接口 breaking，wit 仍 0.2.0——按"版本进接口名"纪律应 bump（漏改事故教训的纪律本义=防静默漂移）。**DK-17.1 已修**：bump **0.3.0** 全链同步（wit+CONTRACT_IFACE+断言+guest 注释；历史教训注记保留原话）。

**A-7（中——悬挂授权）**：unregister 清 manifests/instances/components 三面，**shares 不清**——授权主体注销后授权残留；接收方卸载重装后旧授权自动复活（opt-in 随 manifest 重置但 shares 残留=复活条件仍成立）——fail-closed 缺口。**DK-17.1 已修**：unregister **双向 retain**（from 或 to 任一注销即清）+单测三断言（from 清/to 清/无关保留）。

## 三、SOP 修正（复跑波折归因）

初跑 `e2e_real_wasip2_components_full_chain` 1 挂——根因=**wordcount guest 产物未重建**（WIT init 扩参后 hello 重建了但 wordcount/auditlog 没跟上，旧 wasm 旧签名→type mismatch）。**多 guest 时代 SOP 升级**：build 必须**全 guest**（`for g in $(ls crates | rg guest); do cargo build -p $g --target wasm32-wasip2 --release; done`）再 test——单 guest build 命令退役。

## 四、状态与主线

- **DK-17 L2：关账**；**DK-17.1：完成**（A-6/A-7 关闭）；合约 **0.3.0** 全链一致
- 插件安全模型（docs/plugin-security-model.md）：L0/L1/L2 **三层全落地**；横切不变量五条全在案
- **待堵面清零**（A-1 CallerCtx 已留接口位、A-5 已收口、try_register 随安装面收口——仅剩桌面安装面接线=市场迭代卡锚定）
- **下一批候选（拍板项）**：①桌面安装面接线（市场卡，M——安全模型全链首次触达 UI 面）②T8 bench 基建（待 token）③合约 0.3.0 断言补 guest 侧对齐测试（XS，搭车件可选）
