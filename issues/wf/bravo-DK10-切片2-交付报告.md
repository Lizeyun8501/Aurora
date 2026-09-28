# Bravo DK-10 交付报告 — 切片 2：AI 装配闭环（Rust 面）

> 基线 85fc5c6（rebase 13f0cc5 修补后）· 领地 crates/aurora-ai/** · cloud.rs 切片1 语义零触碰

## 一、做了什么

### 1. 装配入口 `crates/aurora-ai/src/factory.rs`（新增，lib.rs 导出）

`assemble_gated_provider(ollama_base, model, cloud_fallback, resolver) -> Arc<dyn AIProvider>` 签名按任务书冻结原样：

- 主链 `OllamaProvider::new_with_fallback(ollama_base, model, fallback)`（构造不探测——`start_probing` 由生产方对具体类型驱动，见挂起项 ①）；
- fallback 注入：`PolicyGate::new_arc(resolver)` → `with_policy_check(Arc::new(move |ws| gate.guard(ws)))`——**勿双 gate**：只适配 resolver 进 provider 内部 policy_check（切片1 冻结语义），装配层不再包第二层 gate；
- 返回 `Arc<dyn AIProvider>`（AIProvider trait 冻结未动）。

### 2. resolver 写路径 `persist_policy`（policy.rs 扩展，既有签名/契约未动）

`persist_policy(kv, ws_id, policy)`：落 KV `ws-policy:{id}`（b"deny"/b"allow"，与读路径 `eq_ignore_ascii_case(b"deny")` 解析互逆）+ 内存映射同步直写（persist 后 `policy_for` 即时一致）。**读写闭环测试**：persist → 内存即 Deny → 新 resolver `refresh_from` 从 KV 读回同值（模拟重启装配）→ allow 反向同验。

### 3. 端到端装配测试（factory.rs，3 例）

| 测试 | 证明 |
|---|---|
| `deny_workspace_end_to_end_zero_http` | 装配后 Deny 工作区 chat/complete/embed 全 `PermissionDenied` 且 **mockito `matched()==false` 零 HTTP**（mockito 挂着——请求若发出必然命中） |
| `deny_workspace_function_call_blocked_via_chain` | **function_call（Alpha 13f0cc5 修补的第四出网口）经 ollama fallback 链到达 cloud 后仍被拒 + 零 HTTP**——链路端到端 |
| `allow_workspace_fallback_reaches_network` | 主链不可达（未探测 available=false）→ fallback 可达，mockito 命中且响应解码正确（`m.matched()` 实证） |

测试工作区上下文设定方式：装配**前**对 cloud 实例 `set_workspace`（`Arc<dyn AIProvider>` 内同一对象的 RwLock 状态随 move 保持），零 downcast、零 trait 改动。

## 二、本地验证矩阵

| 门 | 结果 |
|---|---|
| `cargo test -p aurora-ai`（CARGO_INCREMENTAL=0） | ✅ **172 passed / 0 failed**（lib 164 = 切片1 160 + persist 闭环 1 + factory 3，doc 8） |
| `cargo clippy -p aurora-ai --all-targets -- -D warnings` | ✅ EXIT=0 |
| `cargo fmt --all -- --check` | ✅ 净（fmt 期间发现 factory.rs root 属主写不进——重建属主后干净，已记 daily） |
| rg 清场 | ✅ `assemble_gated_provider` 仅存在于 factory.rs（定义+doc）与 lib.rs（导出），测试外生产可见；persist_policy 同 |

## 三、挂起项诚实化

1. **装配后主链探测句柄**：`Arc<dyn AIProvider>` 上不可及 `OllamaProvider::start_probing`（trait 冻结无此方法）——生产 AppCore 接入时需具体类型驱动探测，建议切片 3/集成期由装配侧返回句柄或 AppCore 持 `Arc<OllamaProvider>` 后再 as Arc<dyn AIProvider>。**本切片主链可达场景由 ollama.rs 既有探测测试覆盖，装配测试覆盖「主链不可达走 fallback」路径**。
2. **运行时工作区切换**：`set_workspace` 在 `OpenAiCompatProvider` 具体类型上——`Arc<dyn AIProvider>` 无 downcast 通路（trait 无 Any）。生产语义=装配期绑定或 AppCore 持具体类型；UI 动态切换（切片 3）需要时再议最小方案。
3. **缺省 AllowCloud 安全债**：维持 DK-07 冻结契约（复核回执 1ab4068 待议项），本切片不擅动。

## 四、CI

push 后 RUN 号回填（Alpha GitHub API 独立确认口径）。

— Bravo 2026-09-28（DK-10 切片 2）
