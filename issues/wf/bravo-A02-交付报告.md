# A-02 交付报告：附件内置预览器

> 执行: Bravo ｜ 工单: TA 2026-11-10 14:31 ｜ 终 commit: 9e85943（链: 1a9ed8b feat → b5c1d22/9995998/9e85943 收口+帮修）

## 首节 · 开工令假设与代码实况披露

| # | 开工令假设 | 实况 | 处置 |
|---|---|---|---|
| 1 | PDF 降级为「首页缩略+系统打开」 | 首页缩略同样需 pdf.js 渲染引擎——真降级=纯系统打开 | 按工单降级分支落「系统打开卡」;pdfjs-dist 动态 chunk 路径登记 v2（打包依赖非 CDN,~900KB gzip 不入主 bundle,双端 WebView 兼容性先验） |
| 2 | 范围读取=大文件不整块进内存 | AES-GCM 单密文整块解密语义——分片解密不可行 | 解密/校验恒全量（≤20MiB 上限,~10ms 级非阻塞源）;**分片作用于 IPC 传输层**（文本类首片 256KB+按需续片）;诚实注记于命令 doc |
| 3 | 「桌面与移动一致」 | 移动端无附件渲染宿主链（MobileApp 零 attachment 面——移动附件列表 UI 为独立缺口） | shared 组件**依赖注入设计**双端可用（readAttachment/openWithSystem/onExportOffice 全注入位）;移动接线登记 v2（与 E25 APK 二进制桥同列） |

## 实现清单

### AttachmentPreview.tsx（shared/ui-components/src/views,双端复用）
- **四类 MIME 分派**（classifyPreview）: 图片 / PDF / 文本代码 / Office
- **图片**: 缩放 1/2/4x + 旋转 90° 步进 + 原图视窗（新窗口）;blob URL 用后 revoke（unmount/换源双时机——无缓存残留）
- **文本/代码**: 分片按需（首片 256KB×16 封顶=4MB 预算）+「加载更多」续片+行号只读;TextDecoder fatal:false 宽容解码
- **PDF**: 降级卡（系统打开+复制路径+插入引用——零外链,断网可用）
- **Office**: docx/xlsx 扩展匹配 → A-01 导出入口（onExportOffice 注入）+系统打开
- **三动作**: 复制路径（clipboard+prompt 降级）/ 用系统应用打开 / 插入到当前笔记（image `!` 前缀 `attachment://{id}` 引用形态——DK-09 定调一致）
- **无障碍**: role=dialog + aria-modal + Esc 键盘关闭 + img alt + 按钮全可达
- **锁定态**: unseal fail-closed（write_path 唯一入口）→ readAttachment Err → **零内容渲染**（错误文案不含任何附件数据）

### Rust 面（attachment_commands.rs,三命令）
- **cmd_read_attachment +range**: `Option<AttachmentRange>` 向后兼容（旧调用不传=全量）;解密/sha256 校验恒全量 fail-closed;切片越界 clamp 宽容;DTO 加 range 元信息
- **cmd_export_attachment**: unseal→校验→sanitize_filename→dedupe→落盘（Office 导出数据底座——A-01 串联真通路）
- **cmd_open_attachment**: AppHandle+ShellExt Rust 侧 shell open（**不经前端 capability 面**）;临时目录落盘+系统应用原子打开
- lib.rs 注册两新命令

### desktop 挂接
- AttachmentItem 双分支（图片 figcaption/非图片链接卡）加 🔍预览钮
- AttachmentPreviewOverlay 桥: invoke→ReadAttachmentFn + onOpenWithSystem/onExportOffice 真命令接线

## CI 终态（9e85943 实证）
| 门 | 结论 |
|---|---|
| Rustfmt / Clippy / MSRV 1.91 / Wasm×3 | ✅ 全绿 |
| **desktop-check** | **✅ success**（A-02 面终证） |
| Test (stable) | 唯一 FAILED=dk02_s2_trash_flow_roundtrip——**25e7cf4 基线同名同案全链一致**（trash 域=Alpha 面）;**A-02 面（attachment/import/docx/xlsx）零失败** |

## 帮修 Alpha 收口（主干红挡 CI 终证——领地例外披露 ×2）
| 案 | 根因 | 修 | commit |
|---|---|---|---|
| A-08 desktop-check 红 | desktop Cargo 漏挂 aurora-entity 依赖（calendar_commands 引用编不过;**875eb8c 基线实证三门红——A-08 push 未等 CI**） | Cargo.toml 补 `aurora-entity = { path }` | b5c1d22 |
| A-08 Clippy 红 | calendar_vm.rs:39 测试面 unused import（CI `--all-targets` 才现——**本机同款复现定谳,10-16「--all-targets 差异假绿」教训重演实证**） | 删 unused import | 9e85943 |
| A-02 自身两案 | ①参数挂 `///`（unused_doc_comments）②shell open deprecated | ①参数注释降级 ②allow+v2 迁 opener 登记 | 9995998 |

## 验证矩阵
- 本机: clippy workspace（同 CI 参）RC=0 / fmt RC=0 / desk tsc 零错 / shared tsc 我面零错
- CI: 五门绿+Test 分离（上表）
- 行为级测试: AttachmentPreview.test.tsx 已写（四类分派/锁定 Err 零渲染/blob revoke/分片/Esc/插入引用形态断言）——**shared vitest 环境既有坏**（@testing-library/dom 缺——Modal/BlockRenderer/Sidebar 既有测试同跑不动,非我引入,登记 ledger;环境修复即跑）

## 双端影响面
| 端 | 影响 |
|---|---|
| 桌面 | attachment_commands 三命令 + AttachmentItem 预览钮+overlay 桥 |
| 移动 | shared 组件可用（依赖注入）;UI 零改（无宿主链——v2 登记） |
| 共享层 | views/AttachmentPreview.tsx + index.ts 出口 |

## 停手边界遵守
不做 PDF 标注回链 ✅ / 不做图床外链（零外链,断网可用）✅ / 不做附件版本历史 ✅
