# 插件日志前缀约定（DK-14e.1 · 第三方模板价值）

插件 guest 侧的标准输出通道 = **stderr 侧信道**（export-only 合约无返回值
通道；wasmtime 默认透传宿主 stderr——CI 日志可观测、测试可捕获）。

## 前缀约定

| 前缀 | 插件 | 模式 | 输出形状 |
|---|---|---|---|
| `[hello]` | plugin-hello-guest | 分支响应型 | `note.created payload_len=N payload_preview="..."`；未知 event 打 `[hello] WARN unknown event=...`（capability 字段随 0.2.0 资源化退役——token 不透明不可打印） |
| `[wordcount]` | plugin-wordcount-guest | 计算型 | **单行 JSON**：`{"chars":N,"words":N,"lines":N}` |
| `[audit]` | plugin-auditlog-guest | 被动监听型 | `ts=N event=<event> payload_len=<len>`（N=调用计数，1 起递增） |

## 宿主侧审计行（DK-14c.1 起）

## capability.log 转发形态（DK-14c.2 起）

guest 经 WIT import `aurora:plugin/capability@0.2.0` 的自由函数 `log(token, level, msg)` 主动发结构化日志（token 显式 borrow 参数——resource-method 形态经 wit-bindgen 0.57.1 组件生成实证受阻的形态裁决）。宿主 per-method require（granted 含 Log）→ 越权 trap；正常转发 tracing（`plugin` 字段=插件 id），级别映射 `debug|info|warn|error`（未知按 info 兜底）。

**eprintln 侧信道（[hello] 等前缀）保留**：零迁移成本路径不删——stderr 直通对照可观测。

宿主每次 invoke 记审计行到 stderr（`[audit-host]` 前缀）：

`[audit-host] plugin=<id> event=<init|handle-event> result=<ok|denied> [reason=<...>] [event_name=<...>]`

样例：
- `[audit-host] plugin=plugin-hello event=init result=ok`
- `[audit-host] plugin=denied event=init result=denied reason=no-token`（零权限插件越权拦截）
- `[audit-host] plugin=nolog event=log result=denied reason=log-denied level=info`（log 越权——DK-14c.2）
- `[audit-host] plugin=withlog event=log result=ok reason=first-call`（log 首调节流——后续正常转发不记行）

## 判据与规约

1. **测试可断言第一判据**：每行单行、固定前缀、字段 `key=value` 或 JSON——
   宿主侧测试按前缀 grep/解析即可断言（14c capability 落地后可升级为
   返回值通道，stderr 约定保持向后兼容）。
2. **前缀 = 插件标识**：`[短名]`（kebab-case 去掉 plugin-/-guest 后缀）。
   第三方插件遵循同一约定避免多插件日志混淆。
3. **init 行**：`init ok capability="..."`（拒绝则 `ERROR init rejected`）。
4. **结构化优先**：机器可读字段（len/count/ts）> 自由文本摘要。

## 模板工程结构（第三方复制粘贴入口）

三个示范插件=三种模式的可复制模板，共同结构：

- **业务逻辑在 `pub mod logic`**：纯 Rust、无 WIT 依赖，host target 可跑
  `cargo test -p <crate>`（crate-type 含 rlib）；
- **WIT 绑定在 `#[cfg(target_arch = "wasm32")] mod wasm_exports`**：
  `wit_bindgen::generate!({ path: "../aurora-plugin/wit", world: "plugin" })`
  ——单一事实源 `aurora-plugin/wit/`；host 编译零绑定污染；
- **导出面维持 export-only**：`init` / `handle-event` 两函数（14c 合约
  扩展前的稳定面）。
