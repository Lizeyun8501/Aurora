# C-04 交付报告：页面面包屑 + 封面/图标视觉打磨

> 执行: Bravo ｜ 工单: TA 2026-11-18 08:19 ｜ 依赖 A-05 已解（NoteBreadcrumb VM+cmd_breadcrumb 既有）

## 首节 · 工单假设对账
| 工单断言 | 实况 | 处置 |
|---|---|---|
| 「DesktopShell 面包屑仅 3 命中」 | 双实现并存: 树目录壳 nav（folder 面）+A-05 NoteBreadcrumb（编辑器顶栏 3596——A-05 已挂） | 编辑器顶栏常驻已达成——本卡补移动端折叠形态+验证 |
| 「CoverHeader 2.5KB 未与树路径联动」 | 实况=v1 生成式零持久化+恒 72px 色块（每篇占位——正中工单痛点） | 本卡重构（零占位+持久化+选择器） |

## 交付
### ① 面包屑
- 桌面: A-05 NoteBreadcrumb 编辑器顶栏常驻（>4 级折叠「…」首尾各 2+各级可点——单测覆盖）——本卡验证确认
- **移动端折叠形态**: NoteEditor 头部「笔记本名 › …」（pathOfNote 祖先链——localStorage K_TREE 单源;深层只显根笔记本+省略号;点根回知识库;title 全路径提示;次要文字色）

### ② 图标三处一致（DoD②）
- **NoteIcon 上收共享层**（shared/views/NoteIcon.tsx 单一实现）: IconDef+16 def 表+get/setIconDef（aurora.icon.<id> 既有字段不另造）+VivoIcon;mobile primitives re-export（既有 import 面零改动）
- 消费面: 树（E21 既有）+**书签行**（folder→book 档/note→doc 档/search→检索档）+**搜索/最近/收藏列表卡**（NoteCard 头部图标）——同一字段同一实现
- 桌面: CoverHeader 图标=VivoIcon+16 def 选择器面板（role=menu）——与移动端同源

### ③ 封面（CoverHeader 重构）
- 内置渐变图集 8 组（本地 CSS 渐变——零外链 DoD④）+无封面**零占位**（DoD③——v1 恒 72px 色块修根:cover=null 不渲染任何块,首屏正文位置差=0）
- 高度 140px（≤160 上限——视觉克制）
- 持久化: localStorage aurora.cover.<noteId>（{type:'grad',i}|{type:'img',attId}|null）
- **自定义图片**: UI/数据面全落（＋图片钮+file input+onCustomCover prop+attachment:// 渲染挂点）;**附件写入命令缺失**（桌面仅 read/export/open 三读面——core attach_to_note 无 IPC 封装）→ `bravo-request-attach-cover.md` 已提 Alpha（领地纪律不硬改）;落地前按钮禁用+title 披露（禁静默占位——不造假数据调用）

## 验证矩阵
| 面 | 结果 |
|---|---|
| PW c04Cover | **7/7**（折叠屑渲染+全路径 title/树图标同源色值/零外链请求拦截/零 JS 错误） |
| vitest（shared） | **136/136**（+4 新: NoteIcon 字段往返/双载体/16 def 完整性;a05 面包屑套 7 项复证折叠+可点） |
| 回归 | C-01 21/21 · E2E 10/10 |
| tsc | desktop RC=0 / mobile RC=0 |
| DoD③ | 零占位实现=结构保证（cover null 不渲染）;差异 0 行 |
| DoD④ | rg https?:// CoverHeader rc=1（零命中）+PW 运行时零外链请求 |

## 停手边界
不做封面裁剪编辑器 ✓ / 不做主题化封面模板 ✓ / 图标 20px 线性档（VivoIcon 默认）✓
