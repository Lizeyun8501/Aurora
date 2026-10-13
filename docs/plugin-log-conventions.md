# 插件日志前缀约定（DK-14e.1 · 第三方模板价值）

插件 guest 侧的标准输出通道 = **stderr 侧信道**（export-only 合约无返回值
通道；wasmtime 默认透传宿主 stderr——CI 日志可观测、测试可捕获）。

## 前缀约定

| 前缀 | 插件 | 模式 | 输出形状 |
|---|---|---|---|
| `[hello]` | plugin-hello-guest | 分支响应型 | `note.created capability="..." payload_len=N payload_preview="..."`；未知 event 打 `[hello] WARN unknown event=...`；init 拒绝打 `[hello] ERROR init rejected: ...` |
| `[wordcount]` | plugin-wordcount-guest | 计算型 | **单行 JSON**：`{"chars":N,"words":N,"lines":N}` |
| `[audit]` | plugin-auditlog-guest | 被动监听型 | `ts=N event=<event> payload_len=<len>`（N=调用计数，1 起递增） |

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
