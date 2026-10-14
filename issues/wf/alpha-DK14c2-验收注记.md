# DK-14c.2 Alpha 验收注记（关账）

**审核**: Alpha 2026-10-14 10:02-10:25 ｜ **交付**: Bravo ｜ **commit**: 2a9e98a（feat 单 commit）直推 main ｜ **结论**: ✅ 关账，两个形态裁决均实证可辩护，14c.1 台账②（rerun-if-changed）关账

## 终证

- **CI 八门绿**；本地 90 单测+**e2e 6 例全绿**（4 旧+denied+forwards 新 2）
- **产物实证**：wasm-tools 解剖=组件 import 面 `log: func(token: borrow<capability-token>, level: string, msg: string)` 就位
- **rerun-if-changed 本地复核**：pull 后仅跑 cargo build（无 touch）→三 guest 自动重编（1.63s）→wit mtime 驱动确认——**14c.1 假败陷阱根治**（DoD 搭车件 ✓）

## 形态裁决核对（Bravo 两项，均披露+实证）

1. **log 自由函数替代 resource 方法**：wit-bindgen 0.57.1 对 resource-method import 组件生成实证受阻（生成组件 import 面无 log，疑 wit-component 剔除）→自由函数+token 显式 borrow 参数。**语义等价**：宿主 func_wrap 拿 `Resource<CapabilityTokenData>` typed 参数经表 require——token 仍是唯一凭证，"不可伪造/不可转递"威胁模型不变。裁决披露在 WIT 注释+commit ✓
2. **level 用 string 替代 WIT enum**：wasmtime func_wrap 对 WIT enum 需专用 ComponentType derive，string 形态确定性高；宿主四级映射+未知值 info 兜底（level 无安全面，宽容解析可接受）

## 宿主实现核对

- require(Log)→越权 `CapabilityDenied(Capability::Log)`+审计行 `reason=log-denied`（**含 level 字段**，语义准确——14c.1 台账①占位语义在本路径不适用；token-None 路径仍占位，台账①保持）
- 审计节流：per-plugin `log_first_called` 标记，仅首调+越权记行；正常转发 tracing 四级+plugin_id 字段——**日志约定文档 +8 行**
- `Capability::Log` 新变体+字符串映射；d2 HostFunction::Log required_capability 保持 None——**host-invoked 与 guest-invoked 两套语义独立**（宿主自有日志≠插件能力消费），合理

## 注记（流程，非交付问题）

**本地验收 SOP**：cargo test 不编 guest crate（wasm 预构建产物消费模式）——pull 后若涉及 wit/guest 变更，先 `cargo build --target wasm32-wasip2 -p <三插件>`（rerun-if-changed 挂载后例行 1.6s）再 test。本卡本地首跑 denied 例败即此因（旧产物无 log import），CI 无此问题（pipeline wasm job 前置）。

## 流水线状态

**14c 双卡收官**：token 从"收到"（14c.1 生命周期）到"用起来"（14c.2 首个消费端点）——guest-invoked 能力机制闭环。**台账**：①CapabilityDenied token-None 占位（保持）②rerun-if-changed（**关账**）。**后续候选**：14c.3 数据面 guest-invoked（ReadDoc 等——payload 类型设计+数据安全审查先行，建议 Alpha 出设计稿评审后派实现）/marketplace 徽章 UI/T1-T8 台账/14d WIT 前置勘察推进。
