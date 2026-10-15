# DK-16 L1 验收注记 + DK-16.1 派单（2026-10-15 15:47 验收轮）

**交付**: bd8736b ｜ **复核**: Alpha ｜ **结论**: **DK-16 L1 关账**；审计新增 A-4（逃逸面，派小修）/A-5（注记）

## 一、独立复核（本地真跑）

| DoD 项 | 复核 | 结果 |
|---|---|---|
| 前缀拒绝+审计 | storage_guard 三环（wasm_runtime.rs:1122）：token→namespace→method 顺序与设计稿一致；拒绝=CapabilityDenied+reason=namespace-denied/method-denied 差异化审计——**A-2 分级语义对照成立**（能力面 trap） | ✅ |
| query 键集过滤防枚举 | 1113 行 `starts_with(prefix) && contains(pattern)`；e2e=host 预置跨前缀键+guest query 隔离断言（比设计稿"双 id 模拟"更强） | ✅ |
| storage_methods 渐进 | 可选字段缺省 None=全开（1154 行 `if let Some`——存量零改动）；未知 method 串 fail-closed+position（parse 返回 Option→解析层拒） | ✅ |
| C-15a-test 清账 | from_permissions 未知串直接单测双侧（lifecycle+wasm_runtime）+position+valid set 断言——**台账清账** | ✅ |
| 全绿 | 本地 wasip2：**单元 96/96 + e2e 18/18**；clippy 仅 zstd-sys 环境噪声（clang 缺失，非 lint 告警） | ✅ |
| 15b 回归 | print 链独立于数据面，18 例含回归 | ✅ |

## 二、Alpha 审计新增（L1 新面）

### A-4（中危——plugin_id 前缀嵌套逃逸，派 DK-16.1）

`prefix = format!("{plugin_id}:")` + `starts_with`（write/read 面第 1146 行、query 过滤面第 1113 行）：若两插件 id 呈嵌套（`"a"` 与 `"a:b"`），插件 a 可读写 `a:b:xxx`（`starts_with("a:")` 为真），query 列表面同样可见 `a:b:` 前缀键——**read/write/query 三面逃逸**。

- **触发条件**：plugin_id 含 `:` 且呈前缀嵌套——当前 register 全内控（e2e/宿主），无外部输入面 → **不阻塞关账**
- **DK-16.1（XS）派单**：`Plugin::new`/register 面 plugin_id 校验 fail-closed 禁含 `:`（诊断含原串），TS manifest 同步（安装名派生面），单测两例（含 `:` 拒/嵌套 id 组合拒）
- **A-5（注记）**：guest 写死前缀 `plugin-hello:` 与 register 名结构耦合=demo 权宜；真解=WIT self-id 或 init 传参派生前缀——挂 L1 收尾候选（DK-17 议题，不单开）

## 三、状态

- **DK-16 L1：关账**（设计稿 DoD 全过+A-4 不阻塞）
- **DK-16.1：已派（XS）**——plugin_id 禁 ":" + 单测
- **DK-17 候选清单**：A-1（pull_output 调用方鉴权，L2 面）、A-5（self-id 前缀派生）、L2 跨插件授权（share 语义）
- 台账：C-15a-test **清账**；C-ns **关账**（L1 落地）
