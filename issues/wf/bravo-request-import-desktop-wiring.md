# Bravo Request：DK-09 S3 导入器桌面接线（Tauri command 调用点）

> 发起：Bravo · 2026-09-20 · 收件：Alpha
> 依据：DK-09 任务书 §7-2「导入器 CLI/桌面接线（desktop 调用点——跨领地，须先 request）」
> 前置状态：S1（Markdown 目录）+ S2（ENEX）已验收合入 main；附件 API 已批准排期冻结窗口

## 1. 现状与缺口

- `aurora-import` 已提供 `import_markdown_dir` / `import_enex`（`&WriteContext`
  依赖注入，`ImportReport` 已可 JSON 序列化——本切片已铺路）。
- 缺口：**桌面端（apps/desktop/src-tauri，Tauri v2）无任何调用点**；用户
  无法从 UI 触发导入；`attachments/` 相对链接的 webview 渲染路径未定义。

## 2. 提议（desktop 侧，跨领地部分）

### 2.1 Tauri commands（src-tauri 新模块 `import_commands.rs`）

```rust
#[tauri::command]
async fn import_markdown_dir(state, dir: String) -> Result<ImportReportDto, String>;
#[tauri::command]
async fn import_enex(state, file: String, attachments_dir: Option<String>) -> Result<ImportReportDto, String>;
```

- `WriteContext` 由桌面既有 AppCore 装配（`aurora_bootstrap::bootstrap`
  产物已在 Tauri state 中，与笔记写入同源）。
- `ImportReportDto` = `aurora_import::report::ImportReport`（已 `Serialize`）。

### 2.2 附件/资源渲染

- S2 sidecar 模式：`attachments/<file>` 相对链接建议经 Tauri
  `assetProtocol`/`convertFileSrc` 映射到 sidecar 绝对路径（sidecar 目录
  约定 `<data_dir>/imports/<ts>/`，由 command 层生成并写回报告）。
- `attachment://<hash>` 占位链接：**待附件 API（Alpha 冻结窗口切片）落地
  后改为 `aurora://attach/{id}`**，渲染层再接一次即可（一次改动）。

### 2.3 前端

- 文件/目录选择用 Tauri dialog API（前端职责），路径字符串传 command。
- 进度反馈：M2 先返回完整 `ImportReport`（导入千篇量级 < 秒级）；
  流式进度（channel 事件）留 M3 迁移向导。

## 3. Bravo 可承诺

- `ImportReport` 序列化能力：**已交付**（本切片 `dk09_import_report_json_roundtrip`）。
- 接线联调支持：desktop command 侧如需辅助（DTO 字段调整/报告字段增补），
  Bravo 领地内 < 0.5 人日响应。
- 附件 API 落地后的导入器改造（`written_to` → `attachment_id` +
  `aurora://attach/{id}`）：已排 Bravo 队列（Alpha 裁决 < 1 人日）。

## 4. 请裁决

1. desktop command 实现的执行人与排期（建议随附件 API 切片同窗口，
   一次集成两侧改动）。
2. 或授权 Bravo 提 PR 到 apps/desktop/src-tauri（跨领地授权 + 领地边界
   仅限新增 `import_commands.rs` 与 command 注册行）。
3. sidecar 目录约定 `<data_dir>/imports/<ts>/` 是否采纳。
