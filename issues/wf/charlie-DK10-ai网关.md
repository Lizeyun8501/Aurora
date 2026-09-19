# WF-Charlie 任务书：DK-10 AI 网关（aurora-ai 领地）

> 派发对象：Charlie 工作流 agent（独立上下文可执行）
> 生成：2026-09-19 · Repo: Lizeyun8501/Aurora · 基线 commit: 58a2a3c（main, CI passing）

## 你的领地（可修改）

- `crates/aurora-ai/**` —— **全部独占**

## 禁改（改动会被 Alpha 集成时打回）

- `crates/aurora-core/**`（AIProvider trait 已冻结，不改）
- `apps/**`、`crates/aurora-security/**`、`crates/aurora-sync/**`

## 背景

- DK-10 卡（55 人日，M3 P2）：AI 液化与 Agent。issue 清单 §[DK-10]
- 已有：`ollama.rs`（本地）、`cloud.rs`（OpenAI 兼容 fallback）、`mcp.rs`、`registry.rs`、`orchestration.rs`、`policy.rs`（Alpha 冻结的策略契约）

## 第一切片（2–3 人日）：策略接线（DK-07 DoD 2 移交件）

Alpha 已冻结 `policy.rs`：`WorkspacePolicy` / `PolicyResolver` trait / `PolicyGate::guard`（fail-closed，DenyCloud → `Error::PermissionDenied`，测试已过）。

你的任务：
1. 实现 `WorkspaceConfigResolver`（生产 `PolicyResolver`）：从工作区配置读策略——KV `ws-policy:{id}` 键（bytes = "deny" 即 DenyCloud，缺省 AllowCloud）
2. **接入 `cloud.rs::OpenAiCompatProvider` 调用链**：所有出网请求（chat/embed/complete）前过 `PolicyGate::guard(workspace_id)`——注意：现有 provider 方法签名没有 workspace_id 参数，**方案**：provider 构造时注入"当前工作区上下文"（`with_workspace(ws_id)` builder 或 Arc<RwLock<Option<String>>> 状态），guard 不过 → 不发起 HTTP
3. 测试：mock resolver → Private 工作区 `chat()` 返回 `PermissionDenied` 且 **mockito 记录零 HTTP 请求**（证明未触网）；Public 正常请求

## 验收标准

- `cargo test -p aurora-ai` 全绿；clippy --all-targets 零警告
- 测试证明：**被拒请求零网络流量**（fail-closed 证据）
- DK-07 DoD 2「云端 AI 请求对 Private 工作区被阻止」达成

## 后续切片队列（按优先级）

1. DK-10 主体：Agent 液化（orchestration 与 registry 打通真实 provider）
2. 云端请求审计：每次出网写 audit 链（配合 aurora-security::audit_chain）

## 工作流规约（全 WF 通用）

1. 分支：`wf/charlie-ai`，每日 rebase main
2. 提交：`feat(DK-10): ...` / `feat(DK-07): ...`（策略接线用 DK-07 前缀）
3. **本地验证铁律**：clippy --all-targets + `CARGO_INCREMENTAL=0`
4. 集成：每日末推分支，Alpha 次日 merge
5. 接口不够：写 `charlie-request-<主题>.md` 提给 Alpha，禁直改 aurora-core
