# Bravo 派发任务书 — DK-10 切片 2：AI 装配闭环（Rust 面）

> 派发对象：Bravo · 生成：2026-09-28 08:50 · 基线 `85fc5c6`（main）
> 优先级：P1（DK-10 主卡推进面）
> 上游：切片 1（6dc9176）已由 Alpha 复核闭环 + P1 修补（13f0cc5，function_call 补 guard）——复核回执见 `issues/wf/alpha-DK10-切片1-复核回执.md`

## 〇、前置更正（Alpha 撤销催办）

昨夜复核回执与今晨会话中「DK-05M 清理项三次催办（.old.js 残留 + 交付报告缺失）」**信息过时**：
- `.old.js` 已于 09-27 22:57 由你清理（`6d99bc0`，-250 行）；
- 交付报告已随 `78ad4e1` 入仓（`issues/wf/bravo-DK05M-交付报告.md`，七项裁决逐项落地面齐全）。

**你无欠账。** 催办系 Alpha 未核实 git 现状所致，教训已记录（催办前先 `git log --all -- <路径>`）。此条不动你的评分。

## 一、任务范围

生产代码目前**没有任何 AI provider 构造点**（rg 证实：`OllamaProvider::new`/`OpenAiCompatProvider::new` 命中全在测试内）——切片 1 的策略门禁是「造好了闸机但没装到大门」。本切片把装配闭环补齐：

### 1. 装配入口（新增，建议 `crates/aurora-ai/src/factory.rs` 或 lib.rs 内）

```rust
/// Alpha 冻结签名（微调需在交付报告注明理由）：
pub fn assemble_gated_provider(
    ollama_base: &str,
    model: &str,
    cloud_fallback: Option<OpenAiCompatProvider>,
    resolver: Arc<WorkspaceConfigResolver>,
) -> Arc<dyn AIProvider>
```

- 主链 OllamaProvider（`new_with_fallback` 已有，`ollama.rs:185`）；
- fallback 链接 OpenAiCompatProvider（其内嵌 gate 已在切片 1 覆盖四出网方法）；
- **勿重复包 PolicyGate 造成双 gate**——切片 1 的 guard 在 provider 内部，装配层只负责把 resolver 注入 fallback 与（如需）主链工作区上下文。

### 2. KV 读写闭环（`WorkspaceConfigResolver` 扩展）

- 读路径已有：`refresh_from`（policy.rs:90，键 `ws-policy:{id}`，值大小写不敏感 `deny` → DenyCloud，其余/缺失 → AllowCloud——**DK-07 冻结契约，不改缺省方向**）；
- 新增写路径：`persist_policy(ws_id, policy)` → 落 KV `ws-policy:{id}`（值 `b"deny"` / `b"allow"`）。KV store 接口对齐 aurora-core 既有 store 层（`refresh_from` 的键值对来源即该层）；
- 闭环单测：persist → refresh_from 读回 → policy_for 一致。

### 3. 端到端装配单测

- 装配后 DenyCloud 工作区：四出网方法（complete/chat/embed/function_call）全返 PermissionDenied 且 **mockito 零 HTTP**（复用切片 1 `denied_workspace_produces_zero_http` 模式）；
- AllowCloud：主链可达 / 主链不可达时 fallback 可达（mockito）。

## 二、领地与禁改

- **可改**：`crates/aurora-ai/**`（新增 factory/mod 导出、resolver 扩展方法）；aurora-core 若需 KV 辅助，走最小面并在报告注明。
- **禁改**：`crates/aurora-ai/src/cloud.rs` 切片 1 已冻结语义（guard 四处调用 + 测试）；`policy.rs` 既有 trait 签名与缺省 AllowCloud 契约；`apps/desktop/**`（UI 开关是切片 3，Alpha 面，勿越界）。

## 三、DoD（验收门）

1. `cargo test -p aurora-ai` 全绿（新增装配/闭环测试 ≥3 例，全量不回退）；
2. `cargo clippy -p aurora-ai --all-targets` 0 warning + `cargo fmt --check` 净；
3. Deny 工作区端到端**零 HTTP** 实证（mockito assert，不接受仅返回值断言）；
4. 交付报告 `issues/wf/bravo-DK10-切片2-交付报告.md`：方案落地逐项对齐本任务书 + 本地验证矩阵 + 挂起项诚实化；
5. CI 五 job 全绿（Alpha 以 GitHub API 独立确认，不看截图）。

## 四、规约与警告

- commit 前缀 `feat(DK-10):`，直接推 main（流水线既有约定）；
- **半接入三查**（D1 先例）：新代码入仓前 `rg` 清场——装配函数不得仅存在于测试；`assemble_gated_provider` 生产可见（lib.rs 导出）；
- 报告不得复读本任务书——只写「做了什么/怎么验证/什么没做」；
- 有阻塞（KV 接口对不上等）写 `bravo-request-DK10S2-<主题>.md` 提 Alpha，勿自行改冻结契约。

— Alpha 2026-09-28
