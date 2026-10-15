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

## 数据面 guest-invoked（DK-14c.3 起）

三自由函数（bindgen Host trait 挂载——`add_to_linker`+`HasSelf` 形态）：`read-doc(token, doc-id) -> string` / `write-doc(token, doc-id, data) -> string` / `query-docs(token, pattern) -> string`。

**返回原生类型（DK-14c.4 返工——string 协议撤）**：14c.3 曾以 func_wrap ComponentNamedList 受阻为由让步 string 协议化；Alpha 验收实验证伪（bindgen trait 路径下 option/list/result 原生支持，host+guest 全链编译 EXIT=0）——返工为设计稿原令形态：
- `read-doc -> option<string>`：不存在→none（空内容文档可表示）；host-invoked 写的非 string Value 序列化回传
- `write-doc -> result<_, string>`：外层 wasmtime trap（require/存储故障）；内层 Err=协议 err（1 MiB guard→`payload exceeds 1 MiB limit (N bytes)`）
- `query-docs -> list<string>`：键集类型保证（doc-id 可含任意字符——无分隔协议歧义；contains 匹配，非内容搜索）

**共享 KV 语义（安全披露）**：`docs` 无 plugin_id 维度——**全 runtime 插件共享一个 KV 空间**（与 host-invoked ReadDoc/WriteDoc 同一存储，跨调用模式可见——设计使然）。L0 语义可接受（granted per-set+同机互信）；per-plugin namespace 进 14c.4+ 候选。

**审计策略**：write-doc **逐条**（doc_id+size；含 size-exceeded/data-denied）；read-doc/query-docs **首调+越权**（对齐 14c.2 log 先例）；越权逐条 `reason=data-denied`（含 method，不节流）。

guest 经 WIT import `aurora:plugin/capability@0.2.0` 的自由函数 `log(token, level, msg)` 主动发结构化日志（token 显式 borrow 参数——resource-method 形态经 wit-bindgen 0.57.1 组件生成实证受阻的形态裁决）。宿主 per-method require（granted 含 Log）→ 越权 trap；正常转发 tracing（`plugin` 字段=插件 id），级别映射 `debug|info|warn|error`（未知按 info 兜底）。

**eprintln 侧信道（[hello] 等前缀）保留**：零迁移成本路径不删——stderr 直通对照可观测。

宿主每次 invoke 记审计行到 stderr（`[audit-host]` 前缀）：

`[audit-host] plugin=<id> event=<init|handle-event> result=<ok|denied> [reason=<...>] [event_name=<...>]`

样例：
- `[audit-host] plugin=plugin-hello event=init result=ok`
- `[audit-host] plugin=denied event=init result=denied reason=no-token`（零权限插件越权拦截）
- `[audit-host] plugin=nolog event=log result=denied reason=log-denied level=info`（log 越权——DK-14c.2）
- `[audit-host] plugin=withlog event=log result=ok reason=first-call`（log 首调节流——后续正常转发不记行）

## 用户可见输出 vs 诊断日志（DK-15b 分界）

| 通道 | 能力 | 面向 | 载体 | 消费方 |
|---|---|---|---|---|
| `capability-token.log` / HostFunction::log | `log` | 开发者向（诊断流） | stderr 侧信道 + tracing | CI 日志/宿主控制台 |
| `capability-token.print`（DK-15b） | `user-output` | 产品向（用户可见内容） | per-plugin 64KiB 环形缓冲 | 宿主/桌面经 `pull_output` 轮询拉取（拉取即消费） |

- **print 形态**（14c.4 原生类型裁决复用）：`print: func(token: borrow<capability-token>, text: string) -> result<_, string>`——单条超 64KiB 缓冲上限走内层 Err；FIFO 淘汰（`UserOutputBuf`）
- **审计**：print 首调+越权（`useroutput-denied`）留痕；高频调用内容仅 `tracing::debug`（差异化审计节流——print 可能高频，逐条审计会刷爆日志）
- **不回退红线**：stdout/stdin 仍不继承（14e1 收紧有效——插件=不可信代码）；pull_output 为宿主侧 API，不设 manifest 能力
- **T6 钩子**：实时推送落地后 pull_output 可挂 event 通知（现 15s 轮询兼容）

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
