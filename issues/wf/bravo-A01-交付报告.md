# A-01 交付报告：导出补 DOCX/XLSX，导入补 Word/Excel

> 执行: Bravo ｜ 工单: TA 2026-11-09 22:12 ｜ 主 commit: 7599fdc（feat）+ 8eb2252/066e37c（CI 红收口）

## 首节 · 开工令假设与代码不符披露（按真实通路实施）

| # | 开工令假设 | 实况 | 处置 |
|---|---|---|---|
| 1 | 「块模型 → OOXML」 | RawBlock 仅五类（heading/task/code/quote/text），无表格/列表块 | docx_export 输入定稿为**笔记 Markdown 全文**（与 E25 桌面导出同通路，覆盖面含 GFM 表格/列表，大于块模型） |
| 2 | 「导出向导（ImportWizard.tsx 的导出面）新增两格式卡片」 | ImportWizard 实为**导入**向导，无导出面 | 单篇导出门在 DesktopShell ExportMarkdownGate（真实通路）——DOCX 卡加此;XLSX 属集合域加 CollectionView |
| 3 | 「双端 FFI/IPC 暴露」（4 命令） | DoD 明文「移动端可只暴露导出」 | mobile-ffi 暴露 2 导出方法（Vec<u8>）;APK JS 桥 exportFile 现仅 text 通道——二进制落盘需 MainActivity base64 通道（APK 重打不在本卡，已登记 ledger） |

## 实现清单

### 核心（aurora-import 四模块+测试，zip 2.6.1 deflate-only 零重特性）
- **docx_export.rs**: MD→OOXML。标题1-6/正文段/无序有序列表/任务☐☒/GFM 表格/代码块（等宽+浅底纹）/引用（缩进+左框线）;行内粗/等宽;styles docDefaults **eastAsia=宋体**（中文不乱码）;重名 sanitize/dedupe 惯例。**损失清单**: 链接降级 `t (url)` 文本;列表为带符号段落（非 Word 真列表——停手边界内）
- **xlsx_export.rs**: 集合→工作表。Text→inlineStr/Number→n/Date→1900 纪元序列号+numFmt yyyy-mm-dd/Checkbox→b 布尔;**公式列=当前计算值+单元格批注**（comments1.xml+vmlDrawing1.vml+sheet1 legacyDrawing 三件接线——批注文案含表达式）
- **docx_import.rs**: word/document.xml→MD。pStyle Heading1-6→`#`/numPr→列表/w:tbl→GFM 首行表头表格;**图片/文本框→跳过项进报告**（不静默丢）;空文档明确报错
- **xlsx_import.rs**: 首表→集合。列类型推断（Number/Date/Checkbox/Text 全列扫描）;**第2+工作表/无缓存公式→跳过报告**;空表明确报错（禁止静默占位）
- **测试根修三案**（6 测全绿）: ①QName 直比含前缀永不中→绑 LocalName 本体 ②自闭合元素 Event::Empty 三处漏路由（`<sheet/>`/`<Relationship/>`/`<w:pStyle/>`）→Start/Empty 双路由 ③heading 层级算法（空格数当#数）→`1+前缀#数`;④数值统一 f64 消 json!(3)≠json!(3.0) 往返分叉

### 命令面
- **desktop**（export_commands.rs/import_commands.rs+lib.rs 注册）: `cmd_export_note_docx`/`cmd_export_collection_xlsx`/`cmd_import_docx`/`cmd_import_xlsx`。导入走 **write_path 唯一入口**（create_note+save_note_content / set_schema+add_row+set_cell）;跳过项进 ImportReport.warnings
- **mobile-ffi**（udl+impl+uniffi 三面）: `export_note_docx`/`export_collection_xlsx`→Vec<u8>;aurora-import dep 挂载

### UI（无障碍 DoD）
- DesktopShell: 导出门新增 DOCX 格式卡（button 原生键盘聚焦;文案含损失说明）
- CollectionView: 「⬇ 导出 XLSX」钮（aria-label+title+disabled 态——无选中集合禁用）

## ② 挂接清单自查（装配点逐项）

| 装配点 | 状态 |
|---|---|
| aurora-import lib.rs 四 mod 挂载（docx_export/docx_import/xlsx_export/xlsx_import） | ✓ |
| ImportError::new 构造器（report.rs——生成器侧无路径场景） | ✓ |
| zip dep（aurora-import Cargo.toml deflate-only） | ✓ |
| desktop lib.rs invoke_handler 四命令注册 | ✓ |
| desktop export_commands（unwrap/sanitize/dedupe 复用） | ✓ |
| desktop import_commands（import_ctx 同源装配+write_path） | ✓ |
| mobile-ffi api.udl 两方法+uniffi::export impl+aurora-import dep | ✓ 三面同步 |
| DesktopShell ExportFormat 类型/LABEL/doExport/按钮四点 | ✓ |
| CollectionView exportXlsx 回调+按钮 | ✓ |
| a01_office_formats.rs 6 测（含空表拒绝/跳过报告/公式批注断言） | ✓ 全绿 |

## ③ 停手边界遵守说明
- 未做 DOCX 复杂样式双向保真（页眉页脚/样式集/批注读写）——仅读侧 pStyle/numPr 语义还原 ✓
- 未引入任何 Office 运行时依赖（纯 Rust: zip+quick-xml+手写 OOXML）✓
- 未改既有 Markdown/HTML/PDF 导出路径（只增不改）✓
- 发现新债已登记 issues/tech-debt-ledger.md（shared 两 TS6133 他人既有/移动二进制桥通道/Word 真机抽验建议）未顺手修 ✓

## ④ CI 五门
| 门 | 结果 | 证据 |
|---|---|---|
| Rustfmt | ✅ | 8eb2252 check-run success（066e37c 同 diff 面预期同） |
| Clippy (-D warnings) | ✅ | 8eb2252 success+本机 workspace RC=0 |
| MSRV (1.91) | ✅ | 8eb2252 success |
| desktop-check | 8eb2252 红→066e37c 修（set_schema 值传）| 7599fdc/8eb2252 两红均 annotation raw-tail 定谳最小修;066e37c 终态（见下） |
| Test (stable) | 分离披露 | **25e7cf4 基线本身红**: dk02_s2_trash_flow_roundtrip FAILED（trash 域=Alpha 面，annotation 实证，非本卡 diff）;本机 aurora-import 全量 8 目标 RC=0 |

**本机验证**: aurora-import test RC=0（a01 六测+dk09 既有全绿）/clippy workspace RC=0/fmt RC=0/mobile-ffi check RC=0/tsc 改动面零错（全库仅 shared 两 TS6133 他人既有债）

## ⑤ 双端影响面
| 端 | 影响 |
|---|---|
| 桌面 | apps/desktop 四命令+导出门 DOCX 卡+数据库视图 XLSX 钮 |
| 移动 | crates/mobile-ffi 两 FFI 方法（Vec<u8>）;UI 零改（E25 导出分享维持）;APK 二进制桥通道=登记债 |
| 共享层 | shared/ui-components **零改**;aurora-import（core 层）+aurora-core 仅 ImportError::new 构造器追加 |

## 验收 DoD 对照
- 4 命令桌面可用 ✓（移动 FFI 导出 2 方法——DoD 允许）
- 表格/任务/代码块 DOCX 语义 ✓（测试断言 w:tbl/☐☒/等宽底纹+中文 eastAsia）;**Word/WPS 真机开启验证不可行（沙箱无 Office）——结构级测试+规范 OOXML 骨架，建议用户侧真机抽验**（已登记）
- XLSX 列类型正确 ✓（t="n"/t="b"/s="1" 日期样式断言）
- 跳过项报告非静默 ✓（warnings 断言）;空表明确报错 ✓
- 无障碍: 卡片 button 键盘聚焦 ✓/XLSX 钮 aria-label ✓
- CI: fmt/clippy/MSRV ✅;desktop-check 066e37c 收口待终态确认;Test=基线红分离（dk02_s2 非我面）
