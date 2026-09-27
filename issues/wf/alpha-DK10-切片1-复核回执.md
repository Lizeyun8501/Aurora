# DK-10 第一切片 Alpha 复核回执

- **复核人**：Alpha
- **日期**：2026-09-28 07:25–08:30
- **对象**：Bravo 交付 6dc9176（crates/aurora-ai policy.rs +150 / cloud.rs +165 / 交付报告 +31）
- **结论**：**有条件通过（P1 缺口已由 Alpha 当场修补：13f0cc5）**

---

## 一、独立验证（不经 Bravo 声明）

| 项 | 结果 |
|---|---|
| CI 五 job（GitHub API 独立取数） | ✅ Clippy / Rustfmt / desktop-check / MSRV(1.91) / Test(stable) 全 success |
| 本地 cargo test -p aurora-ai | ✅ lib 160 passed / 0 failed + 8 passed（合计 168，与交付声明吻合） |
| clippy（修补后复跑 --all-targets） | ✅ 0 warning |
| fmt --check | ✅ 净 |

## 二、质量审计

**policy.rs ✅**
- 契约注释清晰：fail-closed 决策点在请求发起前（非请求后过滤）；
- `WorkspacePolicy`/`PolicyResolver`（对象安全 trait）/`PolicyGate`（Arc blanket）接口冻结到位；
- `WorkspaceConfigResolver` 预载式（refresh_from → RwLock HashMap → 同步查表）；
- deny 大小写不敏感：`bytes.eq_ignore_ascii_case(b"deny")` ✓，测试覆盖 deny/DENY/junk/missing 四态。

**cloud.rs ⚠️→✅（修补后）**
- embed/complete/chat 三方法 guard 接线正确（available → guard_workspace()? → 出网）；
- `stream_complete` 经 complete 间接覆盖 ✓；
- SSRF 防护（base_url scheme 校验）✓；
- **缺口（P1）**：`function_call` 是第四个出网方法但无 guard——DenyCloud 工作区经 `ollama.rs L493` fallback 链可带 prompt 触网，违背 DK-07 DoD 2「内容永不触网」。Bravo 交付报告与 commit message 均只声明三方法——属范围声明内遗漏，非隐瞒。

## 三、Alpha 修补（13f0cc5）

1. `cloud.rs function_call`：available 检查后补 `self.guard_workspace()?;`（与三兄弟方法对称，guard 调用 3→4 处）；
2. `denied_workspace_produces_zero_http` 测试扩第四断言：DenyCloud 下 function_call 必返 PermissionDenied（aurora_core traits 版 Tool 构造）。

修补后全量验证：test 160+8 全过 / clippy 0 / fmt 净。

## 四、待议安全债（记录，不阻塞）

- **缺省方向**：策略键缺失/非 deny 值 → `AllowCloud`（缺省开放）。此为 DK-07 冻结契约（WorkspaceConfigResolver 键缺省语义一致），Bravo 遵守无误。但从纵深防御视角，「未配置 = 拒绝出网 + 装配层显式预载全部工作区」更安全——留作后续卡提案，不单方面改契约。

## 五、遗留与催办

- **Bravo DK-05M 清理项仍未清**：`apps/mobile` `.old.js` 入仓残留 + DK-05M 交付报告缺失（alpha-DK05M-复核回执 09-27 已列条件，至今未消）——第三次催办；
- DK-10 后续切片（真正策略面接线到 desktop 装配 + UI 开关）待重派或 Bravo 续接；
- 复核过程实证：`tool_calls` token 在部分工具输出层被吞（Read/repr 显示空洞但 in 检测命中 5 处）——**以行号结构 + rustc/CI 裁决内容真实性**，勿信单源 repr。

---
*Alpha 签发 — 13f0cc5 已入 main*
