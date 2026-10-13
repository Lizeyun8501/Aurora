# DK-14c.1 Alpha 验收注记（关账）

**审核**: Alpha 2026-10-13 18:51-19:10 ｜ **交付**: Bravo ｜ **commit**: 48a072f（feat）+03be175（fix）直推 main ｜ **结论**: ✅ 关账，零代码偏差，1 项环境注记

## 终证

- **CI 八门绿**（03be175）：Bravo 自纠 wasm-guest job 三断言 0.1.0 字面量（fix commit 披露"合约版本号是全局字面量，升级时 grep 全仓"教训）
- **本地全绿**：90 单测（fixture 退役 95→90，wat 测试删 5）+**e2e 4 例全过**（真组件 full_chain/instance_cache/unregister_drop/zero_grant_denied，14.4s）+产物 0.2.0 实证（wasm-tools：import capability@0.2.0+export host@0.2.0+resource capability-token）
- WASI 收紧零回退（仅 inherit_stderr）✓

## DoD 核对

| DoD | 结论 | 依据 |
|---|---|---|
| 合约 0.2.0（resource+borrow）编译过 | ✅ | `interface capability { resource capability-token }` 独立接口+world `import capability`（宿主权威）+`use` 复用+`borrow` 参数——设计注释披露威胁模型/borrow 三不可/范围边界 |
| token 全链+越权拦截+审计 | ✅ | 发放=register 时 granted 快照入 ResourceTable（宿主类型 CapabilityTokenData）；传递=typed `(ResourceAny,)`；校验=实例槽前置取 token（None→CapabilityDenied+`[audit-host]` denied 行）；**token-插件绑定靠 per-store 表隔离结构性保证**（跨插件转递不可能）；e2e_zero_grant_invoke_denied 实证 |
| 三插件迁移+e2e 回归 | ✅ | borrow 引用形态+CapabilityToken 类型；wasm 矩阵三 job 绿 |
| derive_capability_string 退役 | ✅ | diffstat wasm_runtime ±321 含退役+linker capability 接口挂载（`linker.instance("aurora:plugin/capability").resource("capability-token", ...)`） |
| fixture 处置 | ✅ | wat×2 退役+平铺回退删除（工单推荐路径执行）；instance_cache 测试迁真组件 |
| 日志约定文档更新 | ✅ | docs/plugin-log-conventions.md +12（[audit-host] 行） |

## 注记（非偏差）

1. **CapabilityDenied(Capability::Storage) 占位语义**：无 token 拒绝≠"拒绝 Storage 能力"，塞 Storage 是 d2 Error 枚举形状约束下的占位。后续卡给 Error 加语义变体（如 `CapabilityDenied { reason: NoToken | Mismatch }`）——**记台账**
2. **本地 wit-fingerprint 陷阱**（环境注记，非交付问题）：wit 文件不在 cargo 依赖图——合约变更不触发 guest crate 重建，本地旧 wasm（0.1.0）致 e2e 首跑 4 例全败（CI 全新 checkout 无此问题）。处置：touch 三插件 lib.rs 强制重编+wasm-tools 实证 0.2.0。**根治建议记台账**：guest crate build.rs emit `cargo:rerun-if-changed=../aurora-plugin/wit`（或 wit-bindgen 宏上游修复后跟进）

## 流水线状态

**Capability 资源化落地**：插件系统安全形态完整——不可伪造 token+默认零权限+越权拦截+审计。**后续卡候选**：①14c.2 token 消费（WIT import 面+guest 主动能力调用——token 真正被"用"起来）②marketplace 徽章 UI 面③Error 语义变体④rerun-if-changed 根治（小，可搭车）。T1-T8 台账未变（T8 与 RV-01 合流待对账）。
