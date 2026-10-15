# 桌面安装面接线（DK-市场卡）——Alpha 2026-10-16

**性质**: M 卡 · 插件安全链首次触达桌面面 ｜ **状态**: 语义层+命令层+UI 管理面落地，本地 107 单元+21 e2e 全绿；tauri 编译/TS 面 CI 桌面 job 验证（本机无 GTK 既有限制）

## 一、架构（三层）

```
UI  PluginsView（列表/卸载/invoke 冒烟；安装表单随市场下载器卡——不做假入口）
      ↓ invoke
命令层  cmd_plugin_install / list / uninstall / invoke（tauri 薄委托+PluginState manage）
      ↓ 纯函数
语义层  aurora-plugin::installer（fail-closed 六环链，KVStore/Marketplace/Runtime 注入——可单测）
```

**六环 fail-closed 链**：①listing 存在性→②manifest.id==listing.id（防错装）→③SHA-256 对账（空 checksum 拒）→④Plugin::new 强签名（15a/16.1 借力：未知 capability/permission+id 禁 `:`）→⑤WasmRuntime::register（二道防线）→⑥KV 持久化（`plugins:installed:{id}`=manifest JSON+`plugins:blob:{id}`=wasm 字节）。

**启动恢复**：setup 里 `restore_installed`（scan→re-register；损坏跳过不阻断启动）。卸载复用 DK-17.1 shares 双向清理。invoke 走 `spawn_blocking`（wasmtime 同步执行）。

## 二、诚实边界

- wasm 来源=本地文件路径（`wasm_path`）；市场下载器（HTTP 拉取+verify_listing）**另卡**——安装语义链已完整，下载器只是字节来源替换
- UI=已安装管理面（列表/卸载/冒烟）；安装表单挂下载器卡（无输入来源不做假入口）
- desktop 编译依赖 GTK——本机 `cargo check` 在 gdk-sys 即止，tauri 面正确性靠 CI 桌面 job（先例：sync_gate/attachment 同路径）

## 三、踩坑记录

1. **错误域转接**：KVStore trait 返回 aurora_core::Error，installer 用 crate::Error——新增 `Error::Kv(String)` 变体（穷尽匹配面安全：唯一 match 带通配）+kv_err 转接 fn；Plugin::new 错误转 InvalidManifest
2. **测试构造**：WasmRuntime 无 Clone（内部 Arc 字段但不 derive）——测试统一 `Arc::new(WasmRuntime::with_default_registry())`+`Arc<dyn KVStore>` 显式上转型
3. **serde 枚举**：RuntimeType 外部形态 `"Wasm"`（非小写）——manifest JSON 测试夹具注意
4. **root 属主文件**：Write 工具产出 root 属主文件后 bash 补丁 Permission denied——统一用 Write/Edit 工具链操作自家产物

## 四、验证与状态

- installer 单测 7/7（成功链/checksum 拒/id 拒/ghost listing 拒/未知 permission 拒+KV 不落/卸载链/启动恢复）——总计 **107/107**；e2e **21/21** 全 guest 重建
- CI 关注点：desktop job（lib.rs 装配段+plugin_commands 四命令+PluginsView/DesktopShell tsc）
