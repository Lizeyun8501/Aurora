# [Alpha→Bravo 派单] DK-导出格式扩展：PDF / HTML（审阅 2.3 收官件）

> 派单方: Alpha ｜ 日期: 2026-10-17 ｜ 前置: MD 单篇+全量+确认门齐（aab8997 链）——剩 PDF/HTML 格式
> 领地: desktop src-tauri export_commands.rs（+前端确认门入口）；core 不动

## 任务 1：HTML 导出（先做——纯文本管线）

- `cmd_export_note_html(note_id, path)`：MD→HTML 转换（轻量：标题/粗斜体/代码块围栏/行内码/链接/列表/引用——**自写有限转换器，不引 md 解析依赖**（编译增量+攻击面考量）——转换不到的语法原样透传，注记诚实边界）
- 内容通路：cmd_export_note_markdown 同款（unwrap 读明文→转换→写盘）；**wiki 链接 `[[title]]` → `<span class="wikilink">[[title]]</span>`**（不解析目标——单篇导出无图谱语义）
- 重名对账：复用 existing_md_names 语义（.html 后缀变体）

## 任务 2：PDF 导出（确认依赖面后实施）

- **先勘察**：tauri 2 print-to-pdf 能力（webview 打印——零新依赖）vs 纯 Rust PDF 库（printpdf 等——重）——**勘察结论回执后再动工**，倾向 webview 打印通路（v1：调系统打印对话框存 PDF——平台能力占位例外需验证）
- 若 webview 通路不可行：**停手回执**（PDF 延后，HTML 先行收官——不引重依赖）

## 任务 3：确认门接入

- HTML/PDF 导出入口**同款确认卡**（「将导出《title》为明文 HTML/PDF」）——aab8997 确认门语义复用；全量导出 v1 不扩 HTML/PDF（单篇先行）

## 纪律

- HTML 转义纪律：**所有文本节点 HTML escape**（&<>"' ——不可信内容 fail-closed，插件输出同款先例）
- 转换器纯函数化（md→html 字符串进出——可单测，CI 桌面面兜底）
- 停手：不做样式主题内嵌（极简内联 CSS 一段）；不做批量 HTML；不做图片内嵌 base64（attachment:// 链接原样+注记）

## DoD

1. 转换器单测（标题/围栏/行内码/转义/[[ 透传）+CI 绿+check/clippy/fmt 三件+注记直推
