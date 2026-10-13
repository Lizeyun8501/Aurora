# DK-14c.2 开工令 · Token 消费首批（WIT import 面+guest 主动调用）

**派发**: Alpha 2026-10-13 21:15 ｜ **Assignee**: Bravo ｜ **Estimate**: 3-5h ｜ **Labels**: `plugin-ecosystem`, `area/wasm`, `area/security` ｜ **依赖**: 14c.1（125f71a 已关账）

## 勘察基线（Alpha 2026-10-13 21:12）

- **两套调用模式对齐**：d2 HostFunction（ReadDoc/WriteDoc/Query/Log+required_capability 映射+validate_capability）是 **host-invoked**（宿主代调宿主函数，JSON args）；本卡补 **guest-invoked**（wasm 内持 token 直调 import 方法，强类型 WIT 签名）——后者才是插件能力消费的本命形态
- **token 本体已带凭证**：CapabilityTokenData { granted: HashSet<Capability> } 在 ResourceTable（14c.1）——resource 方法实现时从表取快照 require，无需新传递通道
- **linker 挂载已就位**：`linker.instance("aurora:plugin/capability").resource("capability-token", ...)`——resource 加方法=同接口扩展
- **合约 0.2.0 无需 bump**：capability 接口加方法=resource 面扩展（0.2.0 内演进；若 Bravo 判断语义 breaking（已有 wasm 兼容性）则 0.3.0，报告披露裁决理由）
- **台账搭车件**：①guest crate build.rs emit `cargo:rerun-if-changed=../aurora-plugin/wit`（14c.1 验收注记②，本卡必须带——防 wit 变更再造假败）②Error 语义变体可顺带（若 diff 小，报告披露；不顺带不硬扛）

## 任务

### 1. WIT：capability-token 加 `log` 方法（首批唯一）

```wit
resource capability-token {
    /// 结构化日志（宿主转发 tracing，带 plugin_id 前缀）。
    log: func(level: log-level, msg: string);
}
enum log-level { debug, info, warn, error }
```

- **裁决边界**：只加 log——数据面能力（ReadDoc/WriteDoc/Query 的 guest-invoked 版）需 payload 类型设计+数据安全审查，属 14c.3+，本卡不碰

### 2. 宿主实现（require 校验+审计+转发）

- resource 方法宿主侧：表取 CapabilityTokenData.granted → **require(Log)**（或 level 映射校验——Log 能力对齐 d2 HostFunction::Log 的 required_capability 映射）→ 越权 `CapabilityDenied`+`[audit-host]` 审计行（reason=log-denied）
- **审计节流**：log 逐条审计会刷屏——审计仅记录**越权**与**首调**；正常转发走 tracing（`debug!/info!` 等，带 plugin_id 字段）——判据：CI 日志可观测不淹没
- L0 语义落地：granted 含对应能力才可调（per-method require）——token 有≠方法全开

### 3. 插件迁移+e2e

- 三插件至少 hello 真调 `capability.log`（init/handle-event 各一）——替代部分 eprintln 侧信道（stderr 保留零迁移成本路径不删——对照可观测）
- e2e 新例：**越权调用**（granted 无 Log 的 manifest→guest 调 log→断言 CapabilityDenied+审计行）+**正常转发**（granted 含 Log→tracing 行可断言或 log capture）
- **搭车件**：三 guest crate build.rs `cargo:rerun-if-changed`（见基线台账）

## DoD

- [ ] 合约 log 方法+wit-bindgen 编译过；wasm-tools 实证 resource 方法面
- [ ] 宿主 require 校验+审计节流+tracing 转发绿；越权 e2e 实证（CapabilityDenied+审计行断言）
- [ ] hello 真调 log；CI 八门绿；WASI 收紧零回退
- [ ] rerun-if-changed 搭车落地（本地 wit 变更→guest 重建实证——改 wit 不 touch 源码→重编发生）
- [ ] 日志约定文档更新（log 方法转发形态）

## 停手边界

- **不做** 数据面 guest-invoked（ReadDoc/WriteDoc/Query——14c.3+，payload 类型+数据安全审查先行）
- **不做** L1/L2 细粒度（token 方法全走 granted 集合 require）
- **不做** marketplace 徽章 UI
- Error 语义变体搭车与否由 Bravo 裁决（报告披露），不顺带不硬扛

## 执行规约

- 独立 commit（可拆 feat+fix）；报告含挂接清单自查节：resource 方法面对照（WIT/宿主/guest 三侧）、审计节流样例、越权 e2e 断言、rerun-if-changed 实证记录
- **单交付线直推 main，不留 wip 分支**
