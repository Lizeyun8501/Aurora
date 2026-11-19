# C-11 交付报告：桌面附件预览接线

> 执行: Bravo ｜ 工单: TA 2026-11-19 23:51 ｜ 依赖 —（A-02 已全量交付）

## 首节 · 工单假设对账
| 工单断言 | 实况 | 处置 |
|---|---|---|
| 「桌面尚未挂接——附件只有列表无预览面」 | **过时**: A-02（1a9ed8b）已随卡交付全链——AttachmentStrip→AttachmentItem（🔍 预览钮）→AttachmentPreviewOverlay 桥→shared AttachmentPreview | 本卡=对账验证+真缺陷修复 |
| 「props 映射对齐 A-02 契约」 | 桥（2201）camelCase ✓ DTO snake_case 直通 ✓ | 核对一致 |

## 真缺陷修复（本卡唯一代码改动——1 行语义）
`AttachmentItem` 内联加载（2088）传 `{ attachment_id: id }`——**Tauri v2 命令参数绑定=JS camelCase**（rust `attachment_id`↔JS `attachmentId`;同文件 Overlay/导出/打开三调用点均 camelCase 唯此处 snake_case）。tauri 模式恒 invalid args→「附件不可读」列表错误（mock 模式恒错态掩蔽——PW 不可达面）。修为 `{ attachmentId: id }`。

## 逐条对账（任务×验收）
| # | 条款 | 结果 |
|---|---|---|
| 1 | 附件区预览入口（图标钮+Enter） | ✅ 既有: 🔍 预览钮（图标+文字超集形态;原生 button=Enter/Space 可达;aria-label 含文件名） |
| 2 | props 映射 | ✅ Overlay: readAttachment(id,range)→invoke({attachmentId,range});DTO 双端 snake_case 对齐 |
| 3 | 锁定态零内容渲染 | ✅ 组件内建（宿主 Err 语义 fail-closed——桥 throw 透传;不自造降级 UI） |
| 4 | PDF 降级系统打开 | ✅ 组件内建 PDF_BUDGET_NOTE;onOpenWithSystem→cmd_open_attachment（camelCase ✓） |
| 5 | tsc 0+build ✓ | ✅ RC=0 / 37s ✓ |
| 验1 | rg 零重复实现 | ✅ jspdf/pdfjs apps/desktop 零命中;预览面板唯一=shared 组件 |
| 验2 | Esc/外点关闭 | ✅ 组件内建（AttachmentPreview 套 7/7 复证） |
| 验3 | 20MB 分片 | ✅ readAttachment range 映射直通;组件内片 256KB×16 封顶 |
| 验4 | 不动 core Rust | ✅ 零 Rust 改动（1 处 TSX 传参名） |

## 验证矩阵
- dtsc RC=0 / vite build ✓ / shared vitest **144/144**（AttachmentPreview 套 7/7）
- 回归: C-01 21/21 · E2E top-paths 10/10
- tauri 运行时面披露: PW=browser-mock（无内核——附件面恒错态）,运行时终证=CI desktop-check+真机

## 总闸六条
1 CI（本帧推送后报）2 零静默占位 ✓（mock 态显式「无附件内核」非伪数据）3 WritePath ✓（零命令新增）4 双端 ✓（mobile 桥既有——Android WebView 面;桌面专属=Overlay 的 Office 导出钮）5 无障碍 ✓（钮原生可聚焦+aria-label;组件 dialog+Esc 套件证）6 报告五项 ✓（本报告）
