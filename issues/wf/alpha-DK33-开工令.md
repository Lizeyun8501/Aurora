# DK-33 开工令 — WASM 插件运行时闭环（派 Bravo）

> 派发：Alpha 2026-10-05 · 依据：迭代排期 e239e71（DK-38 已裁决下迭代再评，本迭代专注本卡+DK-34）

## 一、目标

闭环 aurora-core/l1_infrastructure/wasm.rs 两处 TODO，打通插件最小可用链：

1. **Module 加载**（wasm.rs:48）：从 manifest.entry 路径读取 WASM 字节码 → wasmtime::Module 编译（复用既有 Engine 实例——DK-31 已升 36.0.17）
2. **导出调用**（wasm.rs:83）：wasmtime TypedFunc 调用插件导出函数（最小签名即可，如 `fn(&str) -> String` 或 `fn() -> i32`——以现有 trait 契约为准）

## 二、DoD（验收三件套对齐）

1. 插件加载+调用 e2e 测试绿——**真实 .wasm 样本**（wat 内联编译 `wat::parse_str` 或最小手写字节码，不引入构建期 wasm 工具链）
2. 四门：TEST=0 / CLIPPY=0（--all-targets）/ FMT=0 / DESK=0
3. **不新增配置面**：复用 DK-31 后的 Engine 实例与现有 manifest 结构——边界外不动
4. 交付报告落 `issues/wf/bravo-DK33-交付报告.md` + push

## 三、边界（不做）

- 插件 manifest 体系扩展、权限沙箱面、host function 注入面（后续卡）
- aurora-plugin crate 的注册路由改动（除非加载链必需）

— Alpha 派发
