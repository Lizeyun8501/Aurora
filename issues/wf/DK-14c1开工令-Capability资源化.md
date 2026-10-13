# DK-14c.1 开工令 · Capability 资源化（串→真 token）

**派发**: Alpha 2026-10-13 15:52 ｜ **Assignee**: Bravo ｜ **Estimate**: 6-9h ｜ **Labels**: `plugin-ecosystem`, `area/wasm`, `area/security` ｜ **依赖**: 14e.1（b5cff6b 已关账）

## 勘察基线（Alpha 2026-10-13 15:50）

- **威胁模型**：`capability: string` 可伪造/可猜/可在 payload 夹带转递——guest 间信任边界靠约定不靠机制。真 token=不可伪造句柄（wasmtime ResourceTable），伪造面归零
- **升级路径已预埋**（d1 规划，两处）：
  - `wit/aurora-plugin.wit` 头部注释：`capability: string` → `capability: borrow<capability-token>`（resource）
  - `wasm_runtime.rs:445` derive_capability_string 注释："14c 资源化时替换为真 capability token"
- **基础设施已就位**：WasmStoreCtx 已有 `table: wasmtime_wasi::ResourceTable`（14e.1 WASI 挂载引入）——capability token 同表管理；Capability 枚举 7 项（Fs/Network/Native/Storage/Clipboard/Notifications/Dialog）+Manifest grants/require 雏形在 permission.rs
- **时序背景**：本卡含**扩合约**（14e.1 停手边界预留的"14c 及后续"）；token 消费场景（guest→host 能力调用）需 WIT import 面——**本卡只做形态正确性**（不可伪造句柄发放与校验），import 面/真实能力调用属后续卡

## 任务

### 1. WIT 合约升级（breaking，一次切换）

- `capability: string` → `capability: borrow<capability-token>`（init/handle-event 两参数位）；`resource capability-token` 声明入合约
- 合约头 14c 前置映射注释同步收编（升级完成，映射说明退役）
- 版本语义：aurora:plugin@0.1.0 内 breaking——**升 0.2.0**（semver 诚实；ecosystem 内部无外部消费者，迁移零外部影响）

### 2. 宿主 token 生命周期

- **发放**：register（或 install_component+register 完成时）按 Manifest granted 集合构造 token（rep=granted capability 位集/结构化数据），ResourceTable 登记；**零权限插件发空 token 或不发**（默认零权限语义，预备稿 14c 原意——Bravo 裁决形态，报告披露理由）
- **传递**：invoke 链 derive_capability_string 退役——typed 调用参数从 String 换 resource handle
- **校验+审计**：invoke 入口 require 校验（越权→Error+**审计日志行**，结构化前缀如 `[audit-host]` 入 docs/plugin-log-conventions.md）；每次 invoke 记 token 有效性结果（审计行含 plugin_id/event/结果）

### 3. 三示范插件迁移+e2e 回归

- wit-bindgen 0.57 resource 绑定（borrow 形态 guest 侧签名）——三插件同步升级，业务逻辑不动
- e2e 真组件全链回归（真 wasip2 产物+resource 参数）——CI wasm 矩阵照跑
- **fixture 处置（Bravo 裁决，报告披露）**：合约升级后 d2 手写 wat fixture 与新调用参数类型分裂——推荐 fixture 退役+平铺回退路径删除（export_typed_func 简化，过渡产物无存在必要）；若保留平铺测试需 feature-gate 隔离，不硬扛

## DoD

- [ ] 合约 0.2.0（resource token+borrow 参数）+ wit-bindgen 编译过
- [ ] token 发放/传递/校验/审计全链绿；越权拦截有测试（构造越权 invoke 断言 Error+审计行）
- [ ] 三插件+e2e 真组件回归绿；CI 八门绿
- [ ] derive_capability_string 退役；fixture 处置落地（退役或隔离）
- [ ] WasiCtx 收紧不回退（仅 inherit_stderr，14e.1 成果保持）
- [ ] 日志约定文档更新（[audit-host] 行）

## 停手边界

- **不做** WIT import 面/guest 主动能力调用（token 消费场景属后续卡）
- **不做** marketplace 徽章 UI 面（预备稿 14c 的 UI 部分拆后续卡）
- **不动** 14e.1 的 WASI 收紧成果
- token 权限模型维持 L0（全有/全无 per-granted-set）——L1/L2 细粒度属后续

## 执行规约

- 独立 commit（可拆 feat+fix）；报告含挂接清单自查节：resource 形态对照（WIT/宿主/guest 三侧）、越权测试证明、fixture 处置理由、审计行样例
- **单交付线直推 main，不留 wip 分支**（14e.1 审核教训①，纪律重申）
