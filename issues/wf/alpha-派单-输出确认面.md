# [Alpha→Bravo 派单] 输出确认面：桌面插件输出消费 UI（15b 消费侧欠账收口）

> 派单方: Alpha ｜ 日期: 2026-10-17 ｜ 前置: 插件冻结池定向解冻（用户"继续"按序推进）
> 设计依据: DK-15三案设计稿 15b（缓冲 64KiB/pull_output 拉取/log 分界已落地）——本卡收口消费侧

## 欠账实证

core 面 `UserOutputBuf`（64KiB FIFO）+`pull_output` host API 在（ac8b77d 时点 wasm_runtime.rs）；**apps/desktop 零接线**（rg pull_output/user_output = 0 命中）——插件输出了用户看不到（无人拉取=积压淘汰）。挂起池固化项"输出确认面"的实质=**用户对插件输出行为的知情通道**。

## 任务

1. **数据层**（src-tauri）：`cmd_pull_plugin_output(plugin_id) -> Vec<String>`——core `pull_output` 包装（拉取即消费语义透传）；登记 invoke 面
2. **UI**（桌面）：插件详情页新增**输出区**（15c 徽章页已有路由——复用）：
   - 输出列表（时间序，FIFO 原样呈现）；空态占位「插件暂无输出」
   - 刷新：**手动刷新按钮+15s 轮询**（15b 设计兼容点——T6 实时推送后可换 event，v1 轮询）
   - 面板定位注释：UserOutput=产品向内容（log=诊断流分界照 15b 文档化）
3. **呈现纪律**：输出内容**原样文本渲染**（不 interpret HTML/markdown——插件输出=不可信内容，XSS 面收口）；超长行截断展示+展开

## 边界（停手条款）

- 不做输出确认交互（用户确认后才消费的 gate——v1 知情呈现即达意，确认交互待产品面需求）
- 不做实时推送（T6 后另批）；不做输出导出/过滤
- core 不动（pull_output 语义现成）

## 纪律

- desktop 三查+fmt 强制项；插件输出=不可信内容（渲染面 fail-closed）
- DoD：cargo check+CI 全绿+手动路径自查注记（GTK/浏览器 mock 诚实边界照旧）+注记落档直推
