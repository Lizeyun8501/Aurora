# [Alpha→Bravo 派单] DK-UI-S2：反链面板 + 导出确认门

> 派单方: Alpha ｜ 日期: 2026-10-17 ｜ 前置: S2 验收通过（42af112，双向链接 P0-1 主线收官）
> 领受方式: 按自主领单惯例——领受即建 `bravo-DK-UI-S2.md` 注记开工

## 背景

S2 写路径编排已落地（links:out 权威读/倒排/purge 级联，465 全绿）。本批接 UI 侧两件：**反链面板**（S1 backlinks_of 读取侧首个消费方）+ **导出确认门**（aab8997 v1 边界显式欠账：明文出库确认门归 UI 层）。

## 任务 1：反链面板（数据层 + UI）

- **数据层**：desktop `commands` 新增 `cmd_backlinks_of(note_id)`——`backlinks_of`（S1 原语）→ 解 b64url 标题 → 返回 `[{note_id, title}]`（**snippet v1 不做**——规避锁定态解密面）
- **UI**：笔记编辑视图侧栏/底部面板——反链列表（title 展示，点击跳转打开对应笔记——路由已有）
- **锁定态纪律**：反链只出 note_id+标题（标题来自倒排 b64 键，不触碰内容）——锁定篇天然兼容，无解密路径
- **空态**：无反链显示占位文案（不隐藏面板——保留布局稳定）

## 任务 2：导出确认门

- `cmd_export_note_markdown` / `cmd_export_all_markdown` 调用点前置 UI 确认对话：
  - 单篇：「将导出《{title}》为明文 Markdown（不含附件）」
  - 全量：「将导出 {N} 篇为明文 Markdown（不含附件/trash 排除）」
- **确认门只拦明文出库语义**——不拦路径选择等普通交互
- 导出路径重名对账逻辑保持不动（aab8997 语义）

## 纪律（重申+新增）

- **非阻断**：面板加载失败降级空态，不崩编辑视图
- **desktop 面三查+fmt**：use 导入清单/签名对账/Clone derive——**commit 前必跑 `cargo fmt`（二犯教训，本次为流程强制项）**
- core 不动（本批纯 desktop 面+commands 层）——若需 core 扩展先回执申请

## DoD

1. cargo check（desktop 面本机可验部分）+ CI 全绿
2. 确认门/反链面板手动路径自查注记（本机无 GTK 全验受限——诚实边界照旧）
3. 注记落档+直推 main
