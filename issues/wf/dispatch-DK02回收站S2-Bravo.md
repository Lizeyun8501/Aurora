# DK-02 尾：回收站 S2 派工单（桥+桌面 UI 面）

**派给**: Bravo ｜ **派单**: Alpha 2026-10-28 08:05 ｜ **前置**: 无——**core 全现成零依赖**（与 DK-笔记属性 S2-A 同结构，两卡独立可并行/任意顺序领）

## 侦察基线（core 已完备——勿重写 core）

`crates/aurora-core/src/write_path.rs` 六命令全在案：

- `delete_note`（:1028 软删 trash:{id} 标记+NoteDeleted 事件,幂等重复删无害）
- `restore_note`（:1082） / `purge_note`（:1147 仅回收站内可彻底删）
- `is_trashed`（:1278）/ `list_trashed`（:1302→Vec<TrashedNote> n_ms 排序）
- `purge_expired`（:1317 days 参数自动清理）+ 批量软删族（delete_folder :1667 成员逐个软删）

**缺的只有**：mobile-ffi 桥+MainActivity JNI+桌面 tauri 命令+两端 UI。

## 目标切片

### 1. mobile-ffi 桥（照 S2-A 惯例样板 set_note_encryption:1092）

- `list_trashed_impl` / `restore_note` / `purge_note` / `purge_expired` 四方法 + `TrashedNote` uniffi record
- MainActivity 对应 @JavascriptInterface（JSON 串传递+try-catch fallback）

### 2. 桌面 tauri（新 `trash_commands.rs`——todo_commands.rs:255 样板）

- list_trashed / restore / purge / purge_expired 四命令

### 3. 桌面 UI（知识库视图族）

- 侧栏/视图切换入口：「回收站」视图（list_trashed 渲染：标题+删除时间 n_ms 友好化+恢复按钮+彻底删除按钮带确认）
- 空态文案（「回收站为空」）+「清空回收站（30 天自动清理）」入口
- **恢复后跳转/刷新**（NoteDeleted 对应的恢复事件若 EventBus 有——查 event_bus NoteRestored 类事件,有则听听无则轮询刷新即可,勿造事件）

### 4. 桌面删除入口语义对齐

- 现有删除按钮（知识库列表/右键菜单）确认文案对齐「移入回收站」（不再暗示永久删）
- 若现有 delete_note 调用点已有桌面命令——**对齐文案即可**（命令面不变）

## 边界（停手线）

- write_path.rs 六命令——**勿动**（发现 bug→回执注记）
- mobile UI（回收站视图移动端）——S2-B 模式另卡
- 回收站内笔记的**内容预览编辑**——不做（本卡列表+恢复+彻底删）
- purge_expired 的定时调度——不做（命令暴露即可,桌面「清空」按钮手动触发）

## DoD

1. mobile-ffi uniffi 编译+四方法单测（软删→list_trashed 出现→restore 恢复→purge 永久→list 空 全流程往返）
2. MainActivity 四方法冒烟
3. 桌面 trash_commands 单测+回收站 UI Vitest/Playwright（列表渲染/恢复往返/彻底删带确认/空态）
4. tsc/vitest 全绿+CI 绿；回执 `bravo-DK-02回收站S2.md`

## 已知坑

- **React #310 hooks 纪律**（回收站视图组件）
- TrashedNote 若 core 侧无 Rust struct（list_trashed 返回现构 JSON?）——侦察其返回类型实形,uniffi record 映射以实形为准
- 恢复冲突：恢复同名/同 parent 场景（parent 已删?）——core restore_note 语义若未处理,注记回执勿自行扩 core
- 30 天清理的「天」口径（purge_expired days）——UI 文案与 core 常量对齐（若 core 有默认常量引用之）
