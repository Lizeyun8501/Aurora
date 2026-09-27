# Bravo DK-10 交付报告 — 第一切片：策略接线（DK-07 DoD 2 移交件）

> 基线 1c2dbfb · 领地 crates/aurora-ai/** 独占 · AIProvider trait 冻结未动

## 一、交付内容（原任务书第一切片三项）

| 项 | 落地 |
|---|---|
| `WorkspaceConfigResolver`（policy.rs） | 生产 `PolicyResolver` 实现：**预载式**设计——装配期 `refresh_from(kv, ids)` 从 KV `ws-policy:{id}` 拉策略进内存映射（async KV → 同步查表的桥接），运行时 `policy_for` 同步查；bytes `"deny"`（忽略大小写）→ DenyCloud，键缺失/非 deny → AllowCloud（契约）；`set_policy` 支持运行时直设 |
| `PolicyGate::new_arc` | Arc 共享构造 + `Arc<T>` blanket impl——供装配层把 guard 适配为闭包注入 provider（**对象安全路径，AIProvider trait 冻结零改动**） |
| `OpenAiCompatProvider` 出网链接入（cloud.rs） | ①`workspace: RwLock<Option<String>>` 工作区上下文 + `set_workspace()`；②`policy_check: Option<PolicyCheck>`（type 别名）+ `with_policy_check()` builder；③**embed/complete/chat 三出网方法开头统一 `guard_workspace()?`**——guard 不过 → `Err(PermissionDenied)` 且**不发起任何 HTTP**（fail-closed） |

**设计裁决**（回执说明）：resolver 采用「装配期预载 + 运行时同步查」而非 sync 阻塞读 KV——KVStore 全 async 与 `policy_for` 同步签名的桥接最小解；生产装配层在 bootstrap 期调用 `refresh_from`，运行时管理端变更走 `set_policy` 即时生效。

## 二、DoD 验证（任务书验收标准逐项）

| 项 | 证据 |
|---|---|
| **被拒请求零网络流量** | `denied_workspace_produces_zero_http`：DenyCloud 工区 chat/embed/complete 三路全 `Err(PermissionDenied)`，无任何 HTTP 发起（无 mockito server 挂载） |
| AllowCloud 正常请求 | `allowed_workspace_reaches_network`：mockito 记录命中（`m.matched()`），响应正常解码 |
| 向后兼容 | `no_workspace_context_skips_guard`：None 工作区上下文跳过 guard（既有装配不受影响） |
| KV 语义 | `workspace_config_resolver_reads_kv_policy`：deny/DENY/非 deny 值/键缺失四象限 |
| `cargo test -p aurora-ai` | ✅ **168 passed / 0 failed**（lib 160 + doc 8；新增 5 测试全 ok） |
| clippy --all-targets -D warnings | ✅ EXIT=0（CARGO_INCREMENTAL=0） |
| fmt --check | ✅ 干净（四门槛含 fmt） |

## 三、后续切片队列（任务书既定，待派）

两段式提交（aiLiquify→aiCommit）→ AI 路由器 → MCP 分级鉴权 → Agent 限时/审计/Kill-Switch → 时间轴可视化。

— Bravo 2026-09-27（DK-10 第一切片）
