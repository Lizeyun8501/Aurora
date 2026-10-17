# bravo-导出格式扩展 注记（2026-10-17 领单）

任务: ①HTML 先行（自写有限 MD 转换器——不引解析依赖；escape &<>"' fail-closed；[[透传 wikilink span；重名对账 .html 变体）②PDF 勘察（webview print-to-pdf vs 纯 Rust——不可行停手回执）③确认门同款接入（单篇先行，全量不扩）。

纪律: 转换器纯函数化（可单测）；停手=无样式主题/无批量/无图片 base64。

## 交付

1. **md_to_html 有限转换器**（export_commands.rs 纯函数，零新依赖手写——无 regex）：标题 h1-h6/围栏代码块（未闭合收口）/行内码/粗斜体（贪心配对，嵌套不完美透传）/[text](url) 链接（url escape）/ul+ol 列表聚组/引用/[[title]]→wikilink span（悬挂 [[ 透传）/全文本 escape（& 先转防二次转义）；attachment:// 原样透传（停手条款）
2. **cmd_export_note_html**：通路同款 unwrap→转换→极简内联 CSS 一段骨架→dedupe .html 变体→写盘；**cmd_export_note_pdf**：tauri2 WebviewWindow::print() webview 通路（零新依赖——printpdf 重+排版弱不取）——**语义=打开系统打印对话框用户侧另存 PDF，路径不可编程指定（平台能力占位例外，诚实注记）**
3. **确认门**：单篇三格式按钮（MD/HTML/PDF）——确认卡文案按格式变（PDF 明示"经系统打印对话框"）；全量不扩（停手）

## 单测

export_html_tests 7 例（escape 全五/& 先转/wikilink span+悬挂/标题段落/围栏+行内码/粗斜链表引/wikilink 段内/attachment 透传）——**desktop lib test 链 webkit 本机不可行（fakepc 只给 .pc，链接失败实证）——CI 桌面面兜底（卡面明示路径）**；本机验证=check --all-targets 0+clippy -D warnings 0+fmt 0+TS 0

## 诚实边界

- PDF 路径选择不可编程（v1 webview print 通路——纯 Rust 库不取的代价，注记在案）
- 转换器嵌套强调/表格等复杂语法透传原文（有限转换器 v1）
- 本机单测执行受限（CI Test 面真验）
