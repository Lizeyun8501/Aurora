# Bravo · DK-33 交付报告 — WASM 插件运行时闭环

**Base**: 709f4b6（DK-37 后）｜**执行者**: Bravo｜2026-10-06

---

## 一、卡面两任务落点

| # | 任务 | 落点 |
|---|---|---|
| 1 | Module 加载（wasm.rs:48） | `load()`：`std::fs::read(manifest.entry)` → `wasmtime::Module::new(&engine, bytes)` 编译 → modules 表注册——**复用 DK-31 后 36.0.17 共享 Engine 实例，零新增配置面** |
| 2 | 导出调用（wasm.rs:83） | `invoke()`：TypedFunc 调用链——**最小契约两形态**：args 为 number → `(i32) -> i32`（入参 clamp i32）；否则 → `() -> i32`；JSON 返回 number。per-call 新建 Store+Instance（无 host 状态幂等语义——最小正确；有状态 Store 缓存扩展属后续卡）。`&str/String` 签名需 guest 内存编解码（wit/host-fn 面）——**边界外后续卡**（报告注记） |

**边界遵守**：manifest 体系/权限沙箱/host fn 注入/aurora-plugin 注册路由——零触碰。

## 二、DoD 验证

| 项 | 证据 |
|---|---|
| e2e 真实 wasm 样本 | `dk33_load_and_invoke_e2e`：wat 内联编译（`wat::parse_str`，纯库非工具链）写出真实 .wasm 文件 → load → invoke `answer() -> i32`=42、`bump(41)=42`——**2/2 绿** |
| 错误面 | `dk33_unload_and_missing_entry`：unload 后 invoke 报错；entry 缺失 load 报错 ✅ |
| 四门 | TEST=0 / CLIPPY=0（--all-targets）/ FMT=0 / DESK=0（见 commit） |
| 不新增配置面 | Engine 复用、manifest 结构原样、aurora-plugin 零改动 ✅ |

## 三、依赖注记

`wat = "1"`（workspace.dependencies + core dev-dependencies）——**纯 Rust 文本解析库**（运行时依赖面，非构建期 wasm 工具链，DoD 允许口径）；仅 dev-dependencies 引用，产品编译零增量。
