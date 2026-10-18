# 跨 crate 错误域映射规范（DK-1.2）

> 依据: 审阅报告 1.2「错误域双轨」｜ 2026-10-18 ｜ 状态: 生效
> 方案: 审阅二选一之**「规范文档+增量 #[from] 收敛」**（一次性全量改写禁止）

## 错误域拓扑

```
aurora_core::Error（底座域——KV/网络/加密/写路径）
    ▲ #[from] 直通（增量收敛）
aurora_plugin::Error（插件域——lifecycle/wasm/市场/安装链）
    ▲ From<Error> for core::Error（既有手工映射——语义保留）
aurora_core::Error（回灌——ai_commit 等需要 core 域上报的链路）
```

## 转接规则

| 场景 | 规则 |
|---|---|
| plugin 代码调 core API（KV/写路径） | `?` 自动走 `Core(#[from] core::Error)`（**新代码强制**） |
| 旧 String 转接点（Kv/Network 变体） | **保留兼容不回改**——碰到顺手收敛（渐进，单卡触碰处允许迁移） |
| plugin 域错误上报 core 链路（ai_commit 等） | 走既有 `From<Error> for core::Error` 手工映射（语义保留——NotFound/PermissionDenied/InvalidInput 直通，其余 Internal 化） |
| desktop command 层 | 全部 `map_err(\|e\| e.to_string())` 透传 UI（既有惯例不变——command 层无错误域） |

## 禁止事项

1. **禁止**新代码手写 `map_err(\|e\| aurora_plugin::Error::Kv(e.to_string()))`——用 `?`（#[from] 直通）
2. **禁止**一次性全量改写历史 Kv/Network 调用点（diff 噪声+冻结池区风险——渐进收敛）
3. **禁止**在错误消息里拼接堆栈/敏感信息（KV 键值/路径——log 层职责）

## 1.5 可测性清算注记（同轮）

- **已达成（2026-10-18 交付实践）**：tag_commands（tags_of/tags_match 纯函数）/todo_commands（parse/locate/toggle）/share_commands（b64 往返）/history_commands（键形态）/shortcut_commands（契约护栏）——command 层参数校验/DTO 投影全部纯函数化+单测（CI 兜底）
- **残余**：plugin_commands 11 处 map_err 散点内嵌逻辑（安装链——插件冻结池区，触碰需专项卡）；sync_commands 求值路径（drain 模式——DK-41/47 域）
- **1.7 传输层耦合**（远期）：iroh trait 化——挂起池固化（P2P/WebDAV/S3 可换——等传输需求二现再动）
