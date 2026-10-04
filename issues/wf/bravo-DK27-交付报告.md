# Bravo · DK-27 交付报告 — tags 投影 v2 + SmartFolder tags 条件

**Base**: 804bab2（DK-25/26 验收后）｜**执行者**: Bravo｜2026-10-04

---

## 一、卡面三任务落点

| # | 任务 | 落点 |
|---|---|---|
| 1 | TagsProjection 轻量读取源 | 新文件 `l2_engines/tags_projection.rs`——tag↔note 映射（BTreeMap rows + tags_of 查询 + all_tagged）；事件面 NoteCreated（空 seed）/NoteMetadataChanged.tags（**终态覆盖**）/NoteDeleted（级联）；rebuild 全量灌入；**数据决策：不覆写 apply_batch**——映射为纯内存更新，无 tantivy commit 成本（DK-22 98.2% 占比不成立于此场景），默认逐事件即最优（**不为攒批而攒批**，开工令预置裁决）；bootstrap 注册 + source 拼装（KV note: 扫描 NoteRecord.tags） |
| 2 | FilterRule tags 条件 v2 | `tags_include`（AND 全含）+ `tags_exclude`（任一含即否决）——serde default 兼容；**matches(title) 保留 v1 签名内部转发 matches_full(title, &[])——v1 调用点零破坏**；evaluate_smart_folder 切 matches_full（数据源 rec.tags 与投影同源 KV，一致性由 parity 测试锚定；扫描内零二次查询开销）；**零向量路触碰、零 SearchBackend 改动**（领地声明延续） |
| 3 | UI 接线 | SmartFolderView：包含/排除两输入（逗号分隔多选，AND/排除语义 placeholder 说明）+ 回读/保存扩展；`cmd_smartfolder_set_rule` 扩参（v1 调用方缺省空向量向后兼容）+ get_rule 回读扩展；**新命令 `cmd_set_note_tags`**（write_path.set_note_tags 接线——全量覆盖语义） |

## 二、新增写链（诚实化声明）

`set_note_tags(core, note_id, tags, seal)`：KV NoteRecord.tags 覆盖写（seal 对称）+ NoteMetadataChanged{tags:Some} 发布。**Loro doc meta.tags 双写同步暂缓**——无现成 NoteDoc::open 加载路径；tags 权威源 = KV NoteRecord；doc.add_tag/remove_tag 现无调用者无分叉风险，doc 侧同步待标签编辑 UI 卡（挂账）。

## 三、DoD 验证

| 项 | 证据 |
|---|---|
| 投影一致性 | `dk27_tags_projection_consistency`：set_note_tags ["rust","core"] → catch_up → tags_of 命中；收窄 ["rust"] → core 移除；KV 读取源同落 |
| 过滤正确性 | `dk27_filter_tags_correctness`：include ["rust"]+exclude ["todo"] → a(rust,wip) 命中 / c(rust,todo) 排除否决 / b 无标签不满足 / 自身排除 |
| 批量重建 parity | tags_projection 内联 `apply_batch_matches_rebuild`：逐事件终态 == rebuild 全量终态（双投影同数据 diff=空） |
| 存量兼容 | `dk27_serde_backward_compat`：无 tags 旧 JSON → 空集（诚实化口径锚点）；FilterRule 旧 JSON → 两条件空 |
| 前端 | **TSC=0 / VITE=0** |
| 四门 | 本地自证（见 commit）；CI run 为准 |

## 四、挂账

1. Loro doc meta.tags 双写（待标签编辑 UI 卡——NoteDoc::open 加载路径补齐后同步）
2. tags 权威源单点化后，Notesnap 导出是否含 tags 待产品裁决（不影响本卡口径）
