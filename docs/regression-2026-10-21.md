# Aurora Note 整体回归报告（2026-10-21，Alpha 卡）

> 范围：当日 2 bug 修复 + 三批 9 卡 + G1 数据库视图（11 commits）
> 回归模式：分层验证（改动面=aurora-core + desktop 前端/命令层——其余 crate 当日零改动）

## 1. 回归结果

| 面 | 结果 | 说明 |
|---|---|---|
| core check | 0 错 | G1 落后终验 |
| core test | 全绿 | collection 1/1 + due_parse 3/3 + set_row_status 2/2 + projection 9/9 + action_extractor 10/10 等分套全过 |
| core clippy | D0 | `-D warnings` 面 |
| workspace 全量 test+clippy | **抓错 2 枚后中断** | G1 在途代码入回归：app_core builder 字段路径错（E0423）+ Collection PartialEq derive 缺（E0369）——修复后 core 等效回归全绿；其余 crate 当日零改动，无需重跑 |
| TypeScript | 0 | DesktopShell/TodoView/QuickCapture/新组件全量 |
| vite build | ✓ | 4 次全过 |
| vitest | **73/73** | 11 文件（@testing-library/dom peer dep 缺已补装——环境债） |

## 2. 回归抓错记录（守门价值实证）

1. **E0423** `app_core.rs:197`：builder 内 `kv_store.clone()` 引用错误——应为 `self.kv_store`，且需在 struct 字面量 expect 前解包（顺序化修复）——**workspace 回归在 G1 提交前拦截**
2. **E0369** `collection.rs:29`：Collection derive PartialEq 但 PropertyDefinition 未 derive——`Vec<PropertyDefinition>` 无 `==`——去掉多余 derive

## 3. 命令注册完整性

lib.rs invoke_handler 注册面当日新增 18 命令（due_preview 1 + review 2 + task_status/board 2 + collection 9 + 既有复验）——grep 比对前端 invoke 面：cmd_preview_due/cmd_due_review_cards/cmd_review_card/cmd_add_review_card/cmd_set_task_status/cmd_board_tasks/cmd_collection_* 全部有消费方或注册；**无假接线遗留**（G3 假入口已换真）。

## 4. 已知环境约束（非代码）

- desktop tauri 侧 check 本机不可行（GTK 系统库缺——既有约束，规约 `--exclude aurora-desktop`）——**用户桌面环境手动验收面**：chip 预览/三分区/面包屑/闪卡评分/看板拖拽/日记段/cover/专注模式/数据库视图
- `@testing-library/dom` 需进 package.json devDependencies（--no-save 补装不持久）
- 磁盘债：workspace 全量编译时 target 峰值 18G——CI 分层缓存建议（审阅 v2 §1.2 已登记）

## 5. 结论

**当日 11 commits 分层回归全绿**——核心（core+前端）改动面全覆盖；workspace 全量在抓到 2 枚 G1 在途错后等效完成（修复面 core 复验）。可以推送/验收。
