# bravo-DK-UI-S2 注记（Bravo 2026-10-17 领单）

前置: S2 验收通过（42af112）。任务: ①反链面板（cmd_backlinks_of+EditorPane 底部面板）②导出确认门（aab8997 v1 欠账——单篇/全量入口+确认卡）。

## 设计拍板（自主授权语义）

- 反链项 `{note_id, title: Option}`：引用方 title 读 meta——锁定篇 unwrap fail-closed → title=None → UI 显示 note_id（知有引用不碰内容）；当前笔记锁定 → 空态降级
- 面板位=EditorPane 底部（不动主布局）；空态占位保留；失败降级空态
- 确认门=自制内联确认卡（不依赖 window.confirm——Tauri webview 兼容稳）；单篇入口=EditorPane 标题行；全量入口=侧栏底部（N=UI 列表准确计数）
- 重名对账/导出 cmd 本体零触碰

DoD: check+tsc+三查+fmt 实跑+CI 绿+诚实边界注记+直推。
