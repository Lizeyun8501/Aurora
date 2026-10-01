# Bravo request — DK-02 S3 智能文件夹（开工申请 + 任务书草案）

> 提出人：Bravo · 2026-10-01 · 依据：V26 排期「DK-02 S2+S3（22 人日）」——S2 已验收关闭（alpha-DK02-S2 回执），S3 按排期到期；S2 任务书明令「S3 勿提前做」——**本 request 请 Alpha 冻结裁决后开工**

## 一、卡面任务（清单 DK-02 L416 原文）

「智能文件夹（保存的查询条件）」——动态视图：用户保存一组过滤条件，文件夹内容 = 实时按条件求值（非成员制）。

## 二、Bravo 建议范围（core 面，请裁决）

### 1. 存储（KV）

`smartfolder:{id}` → SavedFilter { id, title, parent_id(树挂载, 复用 S2), sort_order, rule: FilterRule }

FilterRule（v1 最小完备）：
- `title_contains: Option<String>`（子串）
- `tag_any: Option<Vec<String>>`（标签任一命中——对齐 SearchOptions.tag_filter 口径）
- `workspace_id: Option<String>`
- 组合语义：各条件 AND（缺省条件忽略）

### 2. 求值（core）

`evaluate_smart_folder(core, unseal, rule) -> Result<Vec<NoteSummary>, Error>`：scan note:（trash 排除复用）→ 逐条 NoteRecord 按 rule 过滤 → Summary 列表。标签数据源：NoteRecord 有 tags 字段吗——**需核实**（若无则 v1 先做 title_contains + workspace 两条件，tags 进 v2）。

### 3. CRUD + 事件

create/delete/rename/update_rule 走 NoteRecord kind=Folder + rule 字段扩展（**或独立 smartfolder 键制**——**倾向后者**：规则变更不触发笔记投影噪音）；**事件零追加**优先。

### 4. tauri 命令

cmd_smartfolder_create/delete/rename/set_rule/list_items（照 S1/S2 owned-ctx 先例）；UI=Alpha 面。

## 三、DoD 草案

1. 单测：规则求值（AND 各条件/trash 排除/空规则=全部笔记）/ 旧数据兼容 / 级联删除语义裁决点（删文件夹本身≠删成员——**动态视图无成员**，仅删规则+树节点）；
2. core+bootstrap 不回退 + clippy 0 + fmt 净 + mobile 影响面；
3. CI 五 job 绿；半接入三查 tauri 可达。

## 四、预算

3–4 人日（比排期 10 人日低——因复用 S2 树挂载 + trash 过滤 + S1 模式；若 Alpha 要求标签面全量则上浮）。

— Bravo 2026-10-01（等批复即开工）
