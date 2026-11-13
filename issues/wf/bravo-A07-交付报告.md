# A-07 交付报告：集合 / 数据库视图 v2

> 执行: Bravo ｜ 工单: TA 2026-11-13 07:20 ｜ 终 commit: cfdadd5

## 首节 · 开工令假设与代码实况披露

| # | 开工令假设 | 实况 | 处置 |
|---|---|---|---|
| 1 | 「视图配置持久化 db_views 表 M1 新增」 | 集合域本体存 **core KV**（非 SQLite——desktop collections 引擎全程 KV 通道;blocks SQLite 与集合不同域） | dbview:{id} KV 持久化（与集合数据同源同事务域）;db_views SQLite 表待集合域迁移时同步——登记披露 |
| 2 | 「单元格用原生 prompt() 编辑」 | 既有面已部分前移（contentEditable+R10 Select 浮层）——真残余=建集合/加行/导出/加列四处 prompt+导出 alert | 四处全 UI 化（对话框+toast）;**集合流零 prompt/alert 检索确认**;DesktopShell 全局 55 处（非集合流）=独立债登记 |
| 3 | 「看板复用 KanbanBoard.tsx」 | KanbanBoard=Todo 域组件（拖拽逻辑与任务态耦合）;集合看板 v1 已自持分组拖卡 | VM groupRows 共享分组语义+自持拖卡（drop=set_cell）——未硬拆 KanbanBoard（领地+耦合边界） |
| 4 | FilterRule 复用 | core FilterRule={title_contains,tags_include,tags_exclude}（AND 语义——智能文件夹面） | VM 条件模型以 FilterRule 语义为基线（title/tag 条件直接对应）+prop 条件扩展;AND/OR 合成器;未另造 DSL ✓ |

## 实现清单

### 共享 VM（collectionVm.ts——双端同源，DoD「共享同一 CollectionVm」）
- 型识别（serde 外部标签双形态）/格式化（Date 友好化）/比较（Number/Date/Checkbox/中文 locale）
- **matchConds**: 多条件 AND/OR 合成（title/tag/prop×contains/not/eq/ne/gt/lt）
- **sortRows**: 多列稳定排序（首列平→次列决）
- **groupRows**: 看板分组（Select 全选项+未分组兜底）
- **previewConversion**: 类型切换转换预览（样本5+丢失计数——与 core PropertyEngine coerce 对齐）
- **ViewConfig DTO**: 形态/筛选/排序持久化形态
- **单测 9/9**（含 OR 语义负例+拉丁组名排序——中文 locale 序不可假设实证）

### 桌面 CollectionView v2（846→~1250 行重写）
- **零 prompt()/alert()**: CreateDlg/AddRowDlg 对话框+导出默认目录+toast+Schema 面板（名称/类型/选项/必填）
- **四视图**: 表格（语义 table/th）/看板（分组拖卡）/列表/画廊——**切换保筛选+滚动位置（scrollTop 存取）**
- **配置持久化**: cmd_collection_view_get/set（dbview:{id} KV——形态+筛选+排序全存取）
- **筛选**: 快搜（title+文本列）+多条件面板（aria-pressed AND/OR）;**多列排序**（表头点击主序循环,Shift 叠加次序）
- **转换预览**: 类型变更即时样本+丢失计数面板（Text→Number 不可解析置空等）
- **非法值即时回显**: cellErr 红框+横幅（非静默拒绝——core PropertyEngine 拒绝原因透传）
- **行↔笔记锚定**: 行点击跳源笔记+保存事件（aurora:doc-updated——A-05 桥）驱动行刷新（WritePath 事件链,禁互写 ✓）
- **无障碍**: 对话框 role/aria-label/键盘提交 Esc 取消;表头 th;条件钮 aria-pressed

## 验证矩阵
- desktop tsc 零错（labels TS6133=E24 既有非我面）;shared tsc 零错;VM 单测 9/9;cargo fmt RC=0
- CI（cfdadd5）: **desktop-check ✅ / Clippy ✅ / Rustfmt ✅ / MSRV ✅ / Wasm×3 ✅**
- Test: 唯一 FAILED=dk02_s2_trash_flow_roundtrip——**六卡一致基线红**（A-01~07 同案分离）;A-07 面零失败

## DoD 对照与诚实边界
- ✅ 集合流全链零 prompt()/alert()（检索确认——DesktopShell 全局 55 处非集合流=独立债）
- ✅ 四视图切换不丢筛选+滚动位置
- ✅ 类型切换转换预览;非法输入即时错误文案（cellErr+横幅）
- ✅ 桌面移动共享 CollectionVm（shared VM 纯函数域）;**移动默认列表视图**=性能取舍 ADR（表格 contentEditable+浮层在 WebView 触摸面成本>收益——移动列表视图 UI 挂接=后续卡,VM 已就绪）
- ✅ 表格语义 table/th;内联编辑键盘可达（Enter/Esc）
- ⚠️ 登记项: db_views SQLite 表（KV 先行）/移动列表视图 UI 挂接/DesktopShell 全局 prompt 清债/RichEditor 1 处 alert（附件超 2MB 提示）与 AttachmentPreview 1 处（clipboard 兜底）=非集合流小债

## 停手边界遵守
Formula 列只读（不进编辑器——schema 面板类型列表不含 Formula 新建入口）✅ / Relation 不做跨集合双向自动同步 ✓ / 不做实时协同 ✓
