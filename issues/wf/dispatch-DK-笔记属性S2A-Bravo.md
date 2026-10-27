# DK-笔记属性 S2-A 派工单（标签/图标/题头图——桥+桌面面）

**派给**: Bravo ｜ **派单**: Alpha 2026-10-28 07:50 ｜ **前置**: **S1 合入后开工**（note_meta.rs 命令形状以 S1 回执为准——本卡签名按 dispatch 版预定，偏差从回执）

## 背景与侦察基线

S1（core 命令层 8 方法）落库后，打通桥+桌面 UI。**侦察已核**：

- **桌面已有 v1 骨架**：`apps/desktop/src/components/CoverHeader.tsx`（R4-1 生成式渐变+emoji 池 hash 轮换,零持久化——其注记明说「字段落库 v2」=本卡）。v2 语义裁决：
  - **icon**：未设置=不显示（思源语义,删 v1 hash 默认图标）；设置=真持久 emoji
  - **cover**：未设置=**保留 v1 渐变占位**（视觉骨架不倒退）；设置=真图（AttachmentStore sha256）
  - v1 的 spin 会话级轮换逻辑删除（v2 选择器替代）
- **桥惯例三件套**（全现成样板）：
  - mobile-ffi：`crates/mobile-ffi/src/lib.rs` set_note_encryption（:1092）——`pub fn xxx(self: &Arc<Self>, ...) -> T` + `.block_on(aurora_core::...)`
  - MainActivity：`@JavascriptInterface` + core.xxx 直调 + try-catch 恒定 fallback（28 方法在案）
  - 桌面 tauri：`apps/desktop/src-tauri/src/*_commands.rs` 模块（todo_commands.rs:255 样板——`aurora_core::write_path::xxx(&ctx, &note_id, ...)`）

## 目标切片（三面）

### 1. mobile-ffi 桥（uniffi 面）

- `NoteMeta` / `TagCount` **uniffi record**（字段与 S1 Rust struct 一一对应）
- 方法六件：`get_note_meta(note_id) -> NoteMeta`、`set_note_tags(note_id, tags) -> bool`、
  `set_note_icon(note_id, icon: Option<String>) -> bool`、`set_note_cover(note_id, cover: Option<String>) -> bool`、
  `list_all_tags() -> Vec<TagCount>`、`notes_by_tag(tag) -> Vec<NoteSummary>`（NoteSummary 复用现有）
- block_on 模式对齐既有——**勿在 uniffi 面直接暴露失败细节**（bool/空数组 fallback 同惯例）

### 2. MainActivity JNI

- 六方法 @JavascriptInterface（`getNoteMeta/setNoteTags/setNoteIcon/setNoteCover/listAllTags/notesByTag`）
- NoteMeta/TagCount 走 **JSON 字符串**（listNotes 同款——前端 JSON.parse）
- 注记：移动端 UI 消费=**S2-B 另卡**（本卡只通桥——MainActivity 编译+方法冒烟即可）

### 3. 桌面面（tauri 命令+CoverHeader v2+tags chip）

- `note_meta_commands.rs` 新模块（六命令同 S1 签名——ctx 惯例）+ main.rs 注册
- 前端 adapters 平台类型面：`NoteMeta/TagCount` **TS 类型单源**（shared 层或 desktop types——标注 mobile S2-B 复用点）
- **CoverHeader v2**：接真数据（icon 选择器弹层：常用 emoji 网格+清除；cover：文件选图→AttachmentStore 上传→set_note_cover；渐变 fallback 保留）
- **tags chip 行**：CoverHeader 下方（chip 展示+点击弹出编辑——输入回车添加/点 x 删除/去重提示）——**桌面编辑器页接真 core**
- 知识库列表 tag 筛选入口（侧栏 tags 区——list_all_tags 点击过滤 notes_by_tag）——最小版：标签云+选中过滤

## 边界（停手线）

- **mobile 编辑器 UI（属性区 sheet）——S2-B 卡**（本卡移动侧只到 MainActivity 桥）
- note_meta.rs / write_path.rs——S1 已定形勿改（发现问题→回执注记）
- 知识库列表重构/高级筛选语法——不做（最小过滤版）

## DoD

1. mobile-ffi uniffi 编译过+方法单测（假 note 流程：set→get 往返一致；非法 tag 拒）
2. MainActivity 编译+六方法冒烟（mock 桥 smoke）
3. 桌面：`note_meta_commands` 单测（含 S1 校验边界——超长/超数/单 grapheme 图标拒）
4. CoverHeader v2 交互 Vitest/Playwright：设置 icon→持久化（重挂载态保留）；tags 增删往返；cover 设置后渐变 fallback 不再显示
5. tsc/vitest 全绿+CI 绿；回执 `bravo-DK-笔记属性S2A.md`

## 已知坑

- **React #310 hooks 纪律**（CoverHeader v2 弹层组件——hooks 无条件前置）
- 文件选图（desktop）：项目现有文件对话框依赖查一下（attachment 上传桌面入口是否已有——复用勿新引依赖）
- cover 大图：显示层 CSS object-fit 缩放——**勿前端 resize 上传**（存原图 sha256,渲染层裁）
- v1 渐变删除风险：知识库已有用户视觉依赖？——保留 fallback 即无倒退
