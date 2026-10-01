# Bravo 交付报告 — DK-02 S3 智能文件夹（core 面）

> 批复 1aee229（两补点已落）· 领地 write_path + desktop tauri · 预算 3–4 人日内

## 一、批复两补点执行确认

1. **serde 向后兼容**：NoteRecord.rule `#[serde(default)]`（旧数据 None=非智能）；FilterRule 全字段 default；行为级断言：`kind:"SmartFolder"` 无 rule 字段 JSON 反序列化 = SmartFolder/None ✓（smartfolder_delete_no_cascade 尾段）；
2. **结构化过滤不掺向量路**：evaluate 走 KV+rule 过滤（SearchBackend/SearchOptions/HybridSearcher 零触碰）；tags 条件 v1 未做（tags 存 Loro doc meta 无轻量读取源——诚实化，v2 需 tags 投影）。

## 二、交付内容

- **NoteKind::SmartFolder** 变体 + **NoteRecord.rule: Option<FilterRule>**（serde default）；
- **FilterRule v1**：title_contains（大小写不敏感子串）+ AND 语义框架（matches() 单条件实现，扩展点明确）——workspace 条件 v1 裁剪（NoteRecord 无 workspace 字段，单工作区口径，诚实化）；
- **三原语**：create_smartfolder（复用 S2 树挂载+尾部排序）/ update_rule（NoteMetadataChanged 事件，零新事件）/ evaluate_smart_folder_for（scan→排除自身+trash+非 Note→rule 过滤→updated_at 倒序）；
- **rename_folder 扩 SmartFolder**；删除=delete_note 软删复用（动态视图无成员——级联语义行为级断言：删视图不动笔记）；
- **tauri 三命令**：cmd_smartfolder_create/set_rule/list_items（owned-ctx 先例）+ 注册。

## 三、验证矩阵

| 门 | 结果 |
|---|---|
| dk02_s1_trash | ✅ **15/15**（S1 6+S2 5+S3 4：规则求值/trash 排除+动态回归/update_rule+拒绝/级联语义+kind 兼容） |
| core 全量 | ✅ **420 passed**（hybrid 6 含） |
| mobile-ffi | ✅ 17（复现面教训执行） |
| clippy 0（core/bootstrap/mobile）+ fmt 四包 | ✅ |

## 四、挂起项

1. tags 条件 v2（需 tags 投影/轻量读取源）；workspace 条件待多工作区立项；
2. UI 面 = Alpha（列表求值展示/规则编辑器）；
3. 智能文件夹内容变更实时性：求值是拉模式（打开视图时计算）——推送通知未做（v2 可挂投影）。

## 五、CI

push 后回填。

— Bravo 2026-10-01（DK-02 S3）
