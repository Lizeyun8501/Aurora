# 重派任务书 — DK-10 AI 液化与 Agent（原 Charlie，8 天零响应，收回）

> 重派对象：**Bravo**（排队：DK-05M 交付复核后接本卡）
> 生成：2026-09-27 · 基线 f23490a · 派发人 Alpha · 用户指令「同时处理 DK-10 和 DK-11」
> 原任务书 issues/wf/charlie-DK10-ai网关.md（09-19）作废归档；领地契约不变：`crates/aurora-ai/**` 独占，`aurora-core` 的 AIProvider trait 冻结不改。

## 范围（清单卡原文锚点，issues/v26_issue清单.md §DK-10）

1. 两段式提交：`aiLiquify` 提案 → 用户勾选 → `aiCommit` 落库（铁律：AI 不得静默写入）；
2. AI 路由器：Private 强制本地 / Shared 可选云端 / Public 默认云端；
3. MCP 网关鉴权分级：stdio/回环默认信任，对外 HTTP 走 OAuth/HMAC；
4. Agent 会话 15 分钟限时 + 审计 + Kill-Switch；
5. 工具调用时间轴可视化。

DoD：关闭全部 AI 后软件仍完整可用；AI 不在任何核心写入路径；MCP 兼容 Claude Desktop/Cursor；越权操作拦截并记审计。

## 执行要求（补充原任务书）

- **第一切片仍是策略接线**（原任务书 §第一切片，2–3 人日）：`WorkspaceConfigResolver` 实现 `PolicyResolver` + 接入 `OpenAiCompatProvider` 出网链（构造时注入工作区上下文，guard 不过不发 HTTP）——**实现前必须重新实地定位**：`rg -l "PolicyGate|PolicyResolver" crates/` 取真实文件与行号，不得沿用本任务书未给出的路径；
- 后续切片顺序：两段式提交 → 路由器 → MCP 分级鉴权 → Agent 限时/审计/Kill-Switch → 时间轴可视化；
- 每切片独立 commit + 本地验证铁律（clippy -D warnings + CARGO_INCREMENTAL=0）+ 回执文件。

— Alpha 重派 2026-09-27
