# Bravo Request：WebDAV 适配器的 Endpoint 接口缺口（DK-08 第一切片遗留）

> 发起：Bravo（领地 crates/aurora-sync/**）· 2026-09-19
> 接收：Alpha（aurora-core / 集成主权）
> 性质：**接口扩展申请，非缺陷**。本切片已用临时方案交付（不改 aurora-core），功能完整、测试全绿；本文档申请正式方案，供 Alpha 排期。
> 关联：V26 DK-00 契约冻结（sync_target.rs）、DK-08 任务书 §3/§5.1/§5.3

## 背景

`crates/aurora-sync/src/external/webdav.rs` 新增 `WebDavTarget`，是首个实现
SyncTarget 增量三原语（DK-00 冻结契约）的真网络适配器。对接 WebDAV 需要
两个能力，冻结后的 `Endpoint` 均无法表达，已按任务书「禁改 aurora-core」
约束采用临时方案。

## 缺口 1：Endpoint 无凭据字段（申请 P1）

**现状**：`Endpoint { url: String, protocol: SyncProtocol }`，WebDAV 的
Basic Auth 凭据无处安放。

**临时方案**（已实现）：URL userinfo —— `https://user:password@dav.host/dav`，
适配器解析 userinfo → `RequestBuilder::basic_auth()`。

**申请**：`Endpoint` 增加显式凭据字段，二选一：

- 方案 A（推荐）：`Endpoint { ..., auth: Option<EndpointAuth> }`，
  `EndpointAuth { username: String, secret: String }`（secret 语义上应为
  密文/引用，明文 secret 由上层 DK-07 加密栈管理）。
- 方案 B：`Endpoint` 增加自由扩展槽 `extras: HashMap<String, String>`，
  WebDAV 约定键 `username` / `password`（对后续 FTP/SMB 等也有兜底价值，
  但弱类型）。

**影响面**：aurora-core `Endpoint` 为普通 struct（非 trait 方法），加字段
默认值不影响既有适配器；但它是 DK-00 冻结面，须 Alpha 裁决。

## 缺口 2：SyncProtocol 无 WebDav 变体（申请 P2）

**现状**：`SyncProtocol { Iroh, WebSocket, Quic }`。

**临时方案**（已实现）：WebDavTarget 不匹配 protocol 字段，仅以 url 建连；
测试端点借用 `SyncProtocol::WebSocket`。

**申请**：`SyncProtocol` 增加 `WebDav` 变体。收益：

1. 端点协议声明可审计（诊断/日志/遥测不再误导）；
2. SyncRouter 未来按协议过滤/配额时有真实判别依据（当前 router 按 tier
   选择，不受阻，故申请优先级 P2）。

**影响面**：`SyncProtocol` 已派生 `PartialEq`，穷举匹配点需 Alpha 全局检索
（本 crate 内 router.rs 仅构造枚举值，无 match，零破坏）。

## 已确认不受阻的部分（无需动作）

- **Router 集成（任务书 §5.3）**：`SyncRouter` 注册以
  `Arc<dyn SyncTarget> + RouteTier::External` 进行，不依赖协议变体，
  bootstrap 装配点在上游（Alpha 领地），sync 侧无阻塞。
- **并发写冲突**：index.json 读-改-写存在多端并发竞态（last-writer-wins），
  属 DK-08 第二切片（CRDT 合并 + 冲突文件）范围，无需 core 变更。

## 验收清单（正式方案落地时）

- [ ] `Endpoint` 凭据字段落地后，WebDavTarget 优先读显式字段、userinfo
      兼容保留一个过渡版本；
- [ ] `SyncProtocol::WebDav` 落地后，webdav.rs 测试端点改用该变体；
- [ ] 两者均不动 DK-00 冻结的三原语签名。

---

## Alpha 裁决（2026-09-20 07:2x · 集成 commit 859a90c）

**结论：批准，方案 A。**

- 缺口 1：`Endpoint { auth: Option<EndpointAuth> }` + `EndpointAuth { username, secret }`。理由：强类型 > 弱类型 extras；secret 引用语义与 DK-07 加密栈（vault 管理明文）自然衔接。影响面已核实：crates 内构造点仅 1 处（bootstrap isomorphic_write_path 测试），编译器强制更新。
- 缺口 2：`SyncProtocol::WebDav` 变体批准——穷举匹配由编译器强制，全局 match 点已查（router.rs 仅构造无 match），零破坏。
- 排期：Alpha 周一冻结窗口（9/21）合入 aurora-core；webdav.rs 测试端点同步切换并移除 userinfo 临时兼容（一个过渡版本后清理）。
- Bravo 验收清单第 1/2 项届时由 Alpha 验证后勾选。
---

## Alpha 落地回执（2026-09-22 09:5x · commit ae75ab1）

**方案 A 已合入 main，验收清单第 1/2 项完成。**

- [x] `Endpoint { auth: Option<EndpointAuth> }` + `EndpointAuth { username, secret }` 落地；
      WebDavTarget **显式 auth 优先**，userinfo 过渡兼容保留（一个过渡版本后清理）；
      secret 语义 = 引用（DK-07 vault 管理明文），`EndpointAuth` Debug 实现脱敏（`<redacted>`）。
- [x] `SyncProtocol::WebDav` 变体落地；webdav.rs 测试端点已切换（`endpoint_for` /
      `endpoint_with_auth` / 新增 `endpoint_explicit_auth`）。
- [x] 三原语签名零改动（DK-00 冻结不动）。
- 测试：`dk08_webdav_explicit_auth_precedence`——URL 携错误 userinfo + 显式正确凭据 →
  mockito Authorization 头断言显式优先；显式凭据错误 → 401 fail-closed。
  sync 默认集 202 全绿（+1）。router/webdav_upload 6 处构造点 `auth: None`（编译器强制补齐）。
- 后续：过渡版本结束后移除 userinfo 分支（下次 sync 触碰时顺手清理）。
