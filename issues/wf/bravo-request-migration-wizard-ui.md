# Bravo Request：DK-09 迁移向导 UI（防重 / 选择性导入 / 进度）

> 发起：Bravo · 2026-09-21 · 收件：Alpha
> 依据：DK-09 任务书 §7-3 迁移向导 UI；桌面接线 request 裁决问题 2
> （desktop 领地不授权 Bravo 直改 → 本 request 提出调用契约，UI 实现请裁决）
> 内核状态：**已交付同分支**（wizard.rs + 7 个 dk09_wizard_* 测试全绿）

## 1. 内核已就绪（Bravo 领地内，`aurora-import::wizard`）

| 能力 | API | 说明 |
|---|---|---|
| 预扫 | `plan_markdown_dir` / `plan_enex_file` / `plan_opml_file` | 只读扫描 → `ImportPlan { kind, items: Vec<PlanItem{ source, title, content_hash, bytes, resources }>, total_bytes }`，全 serde 序列化 |
| 防重 | options `manifest_dir: Option<PathBuf>` | 会话清单 `<dir>/manifest.json`；键 `(source, content_hash)`；同键跳过（`skipped` 计数）；markdown source = 相对路径，enex = `<文件名>#<idx>`（路径移动不破坏防重） |
| 进度 | options `progress: Option<UnboundedSender<ProgressEvent>>` | 每条目（含跳过）推送 `{ current, total, source }`；total 为过滤后总数 |
| 选择性 | options `only: Vec<String>` | source 白名单（markdown 支持目录前缀）；与 plan.source 同源 |

防重语义（裁决问题 3 的向后兼容）：manifest 为**新增文件**，不改 sidecar /
占位链接路径；附件 API 落地后 imports 会话目录整体迁移方案不变。

## 2. 提议（desktop 侧，跨领地部分）

### 2.1 Tauri commands 扩展（import_commands.rs 增补）

```rust
#[tauri::command] async fn plan_import(source: String, kind: ImportKind) -> Result<ImportPlan, String>;   // 预扫
// 既有 import_markdown_dir / import_enex command 增可选参数：
//   manifestDir / only（前端向导传回）+ tauri channel 桥接 progress
```

### 2.2 向导 UI 流程（apps/desktop 前端）

1. **选源**：文件/目录选择器 → 传路径给 `plan_import`
2. **预览**：展示 `ImportPlan`（条目清单：标题/大小/资源数；按
   `content_hash` 或勾选状态预置 only 集合）
3. **导入中**：订阅 progress channel → 进度条 + 当前条目名
   （`current/total`）
4. **完成**：`ImportReport` 摘要（imported/skipped/failed/warnings 明细）

## 3. 请裁决

1. 向导 UI 实现窗口：随附件 API + 桌面接线同窗口，还是独立切片？
2. UI 组件执行人（Alpha/桌面侧维护者），或授权 Bravo 提前端 PR
   （仅限新增向导页面组件 + command 调用，不触碰既有笔记视图）。
3. 防重键语义确认：`content_hash` 变更即重导（同路径）是否符合预期
   （当前实现）；如需"以源文件 mtime/体积辅助跳过"请指出。

---

## Alpha 裁决（2026-09-21 09:4x · 向导内核验收集成时）

**内核验收：通过**（9 个 dk09_wizard_* 全绿——含 OPML progress+manifest 对齐与进度失败收敛不变量；manifest temp+rename 原子落盘到位）。

**问题 1（窗口）**：command 层（plan_import + 既有 command 增 manifestDir/only/progress 桥接）随附件 API **同窗口**——Alpha 今天冻结窗口作业；前端向导页面为**独立小切片**，随 command 合入后启动。
**问题 2（执行人）**：**授权 Bravo 提前端 PR**——边界仅限：新增向导页面组件 + 对 import_commands 的调用代码；不触碰既有笔记视图与 lib.rs 注册行（注册由 Alpha 随 command 并入）。前提：command 合入后 branch 基于最新 main。
**问题 3（防重语义）**：`content_hash` 变更即重导——**符合预期**，内容级判定优于 mtime/体积辅助（迁移场景源文件变更本应重导），无需增强。

---

## Alpha 验收回执（2026-09-22 · 向导前端 PR）

**验收：通过，已合入 main。**

- **边界合规**：仅新增 `ImportWizard.tsx`（311 行自包含组件）+ `DesktopShell.tsx` 挂载（onImport prop + 条件渲染，不动 MainView 类型面/既有视图）——严格落在授权切片内。
- **质量**：tsc --noEmit 零错、vite build 36 modules 通过；phase 状态机（select→preview→running→done）+ browser-mock 容错（invoke null 不抛错）与 AppShell 语义一致；错误回退到 preview 带错误展示。
- **参数约定**：invoke args 用 snake_case（`manifest_dir`/`only`/`attachments_dir`），与 DesktopShell 既有 `cmd_get_note_content { note_id }` 同款 ✓。
- **提醒**：progress channel 桥接（request §2.1）未接——command 层本期传 None，向导 running 态用 submitting 标志兜底。随下一个切片接（非阻塞）。
- **分支卫生**：`wf/bravo-import-s3` 与 main 同点（旧内核重推），已清理；下次交付前 `git fetch && git log origin/main..HEAD` 自查防重推。
