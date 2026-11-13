# A-10 交付报告：平板双栏布局 + 桌面多窗口（查缺补漏）

> 执行: Bravo ｜ 工单: TA 2026-11-13 09:04 ｜ 主体: c4b9ad9（Alpha 面先行）｜ 查缺: 8bc780d ｜ B-02 重构共存验证 ✅

## 首节 · 盘点结论（TA 指令「已完成部分,依赖 B-02 的查缺补漏」）

| # | 工单项 | 主体状态（c4b9ad9） | B-02 重构后 | 查缺动作 |
|---|---|---|---|---|
| 1 | 平板双栏（≥768 SplitPane 共享层） | ✅ 落（shared 单份+isWide+左 NotesView 右 NoteEditor） | 装配存活 ✓（2872cd6 拆分后 tabletSplit 在位） | 无需 |
| 2 | 分栏持久化（选中 id/栏宽） | ✅ 落（tabletSelection localStorage+storageKey 栏宽） | 存活 ✓ | 无需 |
| 3 | 旋转保持（选中+光标） | 选中 ✅（editing state）;**光标缺口**——跨 ≥768 边界（如 iPad mini 竖 744px）tabletSplit↔单栏切换**重建编辑器 DOM 丢 selection** | — | **8bc780d 修**：auroraEditor selectionCache（noteId→selection JSON LRU16;destroy 存/mount 恢复——loro 首帧同步后 setSelection+focus;文失配静默降级）;options.noteId 通道 RichEditor+桌面双点透传 |
| 4 | 桌面多窗口（共享 core） | ✅ 落（cmd_open_note_window——static APP_STATE 同进程同 core/WritePath,**结构天然禁各自写模型**;#note 路由+⧉ 入口+命令面板） | 存活 ✓（2d6fb40 撞车修复后） | 无需 |
| 5 | 并发编辑 CRDT 合并 | ✅ 落（cmd_save_note_snapshot apply_update 合并语义+core crdt_concurrent_merge 测试在册） | — | 无需 |
| 6 | 关闭未保存提示 | **部分**——__auroraDirty（PM 编辑面）✅;**源码态（A-03）缺口**——源码 textarea 改动不经 PM dispatch→不置 dirty→关窗**静默丢弃**（工单原文明示） | — | **8bc780d 修**：MarkdownSourceEditor 加 onDirty（CM updateListener docChanged 首变更上报）;桌面 __auroraSrcDirty 信号（apply 成功/discard 清）;onCloseRequested 双信号消费（confirm 文案含源码模式明示,确认后双清防环） |

## 验证矩阵（8bc780d）
- shared/desktop tsc 零错;回归 16/16（a05+a07 面）;cargo fmt RC=0
- CI: **desktop-check ✅ / Clippy ✅ / Rustfmt ✅ / MSRV ✅ / Wasm×3 ✅**
- Test: 唯一 FAILED=dk02_s2_trash_flow_roundtrip——**七卡一致基线红**（A-01~A-10 同案分离）;A-10 面零失败

## DoD 对照
- ✅ 平板旋转后选中笔记保持（editing state）;**光标位置保持**（同 noteId 重挂载 selectionCache 恢复——8bc780d 补）
- ✅ 两窗口同笔记并发编辑合并无丢（apply_update CRDT——core 测试在册）
- ✅ 关窗未保存有提示（PM 脏+**源码态脏双信号**——8bc780d 补全工单原文面）

## 诚实边界
- 移动端源码态返回提示未扩面（工单「窗口关闭」语义=桌面窗口;移动 App 内导航另卡）
- selection 恢复=同 noteId 重挂载场景（跨笔记切换不恢复——语义正确性优先,非笔记专属光标不串）
- 多窗口并发=CRDT 合并语义（不做 OT/光标级协同——停手边界「不做实时协同」遵守 ✓）

## 停手边界遵守
不做窗口级独立工作区 ✓ / 不做跨窗口拖拽块（v2）✓ / 不做实时协同编辑 ✓
