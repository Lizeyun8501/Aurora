# Bravo 派发任务书 — DK-02 S2：目录树与原位还原（core 面）

> 派发对象：Bravo · 生成：2026-09-28 19:45 · 基线 `66d7f9e`（main）
> 优先级：P1 · 预算：12 人日 · 前置：DK-02 S1 回收站（f175692/57c8f95 已闭环）
> 注意：**DK-03 S1 向量基建（你手上）不受影响**——本任务书入库排队，完成 DK-03 S1 后开工。

## 〇、两项架构改判（Alpha 裁决，理由入档）

1. **递归 CTE + 复合索引 → 内存树组装**：卡面写 `notes.parent_id` SQL 表前提，但现实主存储是
   KV（`note:{id}` JSON 值）——递归 CTE 不可用。改判：**全量元数据加载 + 内存树组装**
   （O(N)，万条级 list_notes 已是全量加载先例）+ `(parent_id, sort_order)` 内存 BTreeMap 排序。
   递归 CTE 挂起至存储 SQL 化（远期，不入排期）。
2. **文件夹删除 = 内部笔记批量入回收站**（复用 S1 trash 标记 + discard 语义），不做物理级联——
   与「误删即永久损失」的产品立场一致。

## 一、S2 范围（core 面）

### 1. NoteRecord 扩展（serde default 兼容旧数据——无字段旧笔记 = 根下笔记）

```rust
pub parent_id: Option<String>,   // None = 根
pub kind: NoteKind,              // Note | Folder（serde default = Note）
pub sort_order: i64,             // 同父下排序（serde default = 0）
```

### 2. 树 CRUD（write_path 新函数）

- `create_folder(ctx, parent_id: Option<&str>, title) -> WriteReceipt`；
- `move_node(ctx, note_id, new_parent_id: Option<&str>, sort_order)` ——
  **环检测**：沿 new_parent 链上溯（depth 上限 = 节点总数），命中自身子树 →
  `Error::CircularMove`（可读提示，DoD 1）；
- `rename_folder(ctx, note_id, title)`；
- `list_tree(core) -> Vec<TreeNode>`（id/kind/title/parent_id/sort_order/children 嵌套或扁平
  由 UI 组装——**选扁平+排序键**，UI 端组装更利于虚拟滚动）；
- `delete_folder(ctx, folder_id)`：**递归收集子树全部笔记**（含子文件夹）→ 批量入回收站
  （复用 S1 trash 标记 + NoteDeleted 事件每笔记照发）→ 文件夹节点标记删除；
- **事件**：复用既有 `NoteMetadataChanged`（树移动/重命名=元数据变更）——**不加新事件**
  （字典冻结面）；目录树不进搜索索引（零投影改动）。

### 3. 回收站原位还原（DoD 2 — S1 标记键升级）

- `trash:{id}` 值结构追加 `origin_path: Option<String>`（如 `"工作/项目A"`）——
  **S1 旧标记兼容**：无该字段视为 origin_path=None（还原到根，UI 显示「（原位置未知）」）；
- `restore_note` 升级：恢复时若 origin_path 各级父仍存在 → 回原位（parent_id + 尾部 sort_order）；
  父缺失 → 挂根 + trash 条目 UI 标注（core 面只保证 parent_id=None + 文档注明）；
- delete_note 升级：写标记时快照当时路径（沿 parent 链上溯拼 title）。

### 4. tauri 命令（照 S1 先例）

`cmd_create_folder / cmd_move_note / cmd_rename_folder / cmd_list_tree`——
**cmd_delete_note 语义升级说明**：若目标是 Folder → 走 delete_folder 级联。

## 二、禁改与边界

- **禁改**：S1 回收站已闭环行为（restore/purge 语义只增不改）；投影/搜索；移动端（下片）；
  事件字典（零追加）；
- **UI 挂起**：桌面树形侧栏是 Alpha 后续切片（本卡交付 core + tauri 面）；移动端更后；
- **标签/智能文件夹/书签**：卡面其余三任务**不在 S2**（S3 及以后切片，勿提前做）。

## 三、DoD

1. 单测：环检测（自挂/子树挂/祖先链三态拒绝+可读 Error 文案断言）/ 树组装排序 /
   原位还原（父存/父失两态）/ 旧数据兼容（无新字段 NoteRecord 反序列化）/
   文件夹级联入回收站（N 笔记 + 子文件夹嵌套）；
2. 全量 `cargo test -p aurora-core -p aurora-bootstrap` 不回退 + clippy 0 + fmt 逐包净；
3. CI 五 job 绿（Alpha API 独立确认）；**复现面教训执行**：新增测试同时跑 mobile-ffi 影响面
   （list_notes 若涉树字段）；
4. 半接入三查：tauri 命令可达（非仅测试）；交付报告：改判确认 + 旧数据兼容策略 + 挂起项。

— Alpha 派发 2026-09-28
