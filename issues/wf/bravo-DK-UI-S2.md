# bravo-DK-UI-S2 注记（Bravo 2026-10-17 领单）

前置: S2 验收通过（42af112）。任务: ①反链面板（cmd_backlinks_of+EditorPane 底部面板）②导出确认门（aab8997 v1 欠账——单篇/全量入口+确认卡）。

## 设计拍板（自主授权语义）

- 反链项 `{note_id, title: Option}`：引用方 title 读 meta——锁定篇 unwrap fail-closed → title=None → UI 显示 note_id（知有引用不碰内容）；当前笔记锁定 → 空态降级
- 面板位=EditorPane 底部（不动主布局）；空态占位保留；失败降级空态
- 确认门=自制内联确认卡（不依赖 window.confirm——Tauri webview 兼容稳）；单篇入口=EditorPane 标题行；全量入口=侧栏底部（N=UI 列表准确计数）
- 重名对账/导出 cmd 本体零触碰

DoD: check+tsc+三查+fmt 实跑+CI 绿+诚实边界注记+直推。

## 交付（f175eeb，CI 8/8 绿 2026-10-17）

| 任务 | 落实 |
|---|---|
| 反链数据层 | backlink_commands.rs：cmd_backlinks_of——note_id→标题（锁定 fail-closed→空集降级）→S1 backlinks_of→引用方 title 补全（锁定篇 title=None，不触碰内容）；查询失败 warn 降级 |
| 反链面板 | BacklinksPanel（EditorPane 底部位）——加载/空态占位/点击 onSelect 跳转（主组件 setSelectedId+setView 链）；锁定引用方显示 (锁定)+短 id；失败标注降级 |
| 导出确认门·单篇 | ExportMarkdownGate：EditorPane 标题行按钮+确认卡「将导出《title》为明文 Markdown（不含附件）」 |
| 导出确认门·全量 | Sidebar「导出全部」入口（N=UI 列表准确计数）+主组件 overlay 确认卡「将导出 N 篇…trash 排除」 |

## 验证矩阵

| 门 | 结果 |
|---|---|
| TS（tsc --noEmit） | 0 |
| fmt 实跑+check | 0 |
| fakepc DESK check（本机可验面） | 0 |
| CI 8/8 | desktop-check/Clippy/Test/Rustfmt/MSRV/3×guest 全绿 |

## 诚实边界

- 本机无 GTK：面板/确认门交互路径未真跑（CI desktop-check 编译面绿兜底）——手动路径自查=代码级走查（invoke 参数/空态分支/降级 catch 全覆盖）
- v1 路径选择=prompt 预填默认目录（Tauri dialog 插件未接入——卡面「不拦路径选择」语义满足；原生文件对话框另批）
- 全量 N=UI 列表计数（与导出 cmd 实际扫描同源 scan_prefix note:，并发写窗口可能 ±1——知情同意语义足够）
- browser-mock（invoke=null）模式：确认门显示不可用提示、反链面板空态——不崩视图
