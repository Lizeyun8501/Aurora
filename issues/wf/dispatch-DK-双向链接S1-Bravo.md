# DK-双向链接 S1 派工单（L 卡首切片，core 面）

**派给**: Bravo ｜ **派单**: Alpha 2026-10-16 13:46 ｜ **来源**: 深度审阅 P0-1（docs/software-design-review-2026-10-16.md §2.1——最大功能缺口）

## 目标（本切片：core 解析+索引原语，UI 另切片）

笔记内容中的 wiki 链接 `[[$title]]` 提取 → **链接边索引**（出链/反链双向可查）→ KV 持久化。命令层/反链面板 UI/图谱=S2/S3 另排。

## 设计要点

1. **新模块 `crates/aurora-core/src/links.rs`**（你领地）：
   ```rust
   /// 从 markdown 正文提取 wiki 链接目标（去重保序）。
   /// 语法面：[[target]] / [[target|alias]]——target=笔记标题（v1 按标题解析，
   /// id 解析 S2 随命令层）；忽略代码块 ``` 围栏与行内 ` ` 内的 [[]]。
   pub fn extract_wiki_links(content: &str) -> Vec<String>;
   /// 链接边（索引载荷）。
   pub struct LinkEdge { pub src_note_id: String, pub target_title: String }
   ```
2. **索引原语**（KV 键域，登记进 KV capability 设计稿保留前缀清单）：
   - `links:out:{src_note_id}` = serde_json(Vec<String>)（出链标题集——**全量覆写**，删旧写新）
   - `links:in:{target_title_b64}` = serde_json(HashSet<note_id>)（反链倒排——**增量维护**：读旧集→apply diff→写回）
   - b64=urlsafe no-pad（标题含中文/斜杠安全）
3. **维护语义**（写路径钩子，原语函数供调用方编排）：
   ```rust
   pub async fn reindex_note_links(kv, note_id, old_content, new_content) -> Result<(), Error>
   ```
   ——出链 diff（旧出链集 vs 新出链集：新增目标进倒排+删除目标出倒排）；**rename/删除笔记的级联**（目标消失时悬挂边处理）：v1 反链查询时**惰性校验**（倒排里的 note_id 查 existence，悬挂标记 stale 不展示）——不做主动清扫（S2 决策）
4. **单测**（≥8 例）：基础提取/alias/代码块忽略/行内码忽略/中文标题/出链全量覆写（删链后出链集收敛）/倒排增量（加链+删链 diff）/悬挂边惰性语义
5. **e2e 不强求**（core 面单测完备即可；S2 命令层接 e2e）

## 边界（停手线）

- **installer.rs/audit.rs/hot_update.rs/cache.rs——Alpha 领地勿动**（hot_update 统一并行推进中）
- write_path/sync 集成（保存时自动 reindex）——S2（涉及写路径编排，需 Alpha 复核接入点）
- 图谱可视化/反向链接 UI——S3
- id 解析（[[note-id]] 形态）——S2 随命令层定 UI 语法

## DoD

1. 单测 ≥8 例全绿+既有 127/22 零回归；clippy/fmt 零告警
2. 注记 issues/wf/bravo-DK-双向链接S1.md（语法边界决策/键域形状/悬挂语义）
3. push main（惯例直推）

## 已知坑

- **KVStore 错误域**：aurora_core::Error——links.rs 在 core 内**直接用本 crate Error**（无需转接——installer 的 Error::Kv 转接是跨 crate 场景，你不需要）
- 中文标题 b64 编码：urlsafe no-pad，decode 侧容错（非法 b64→NotFound 语义）
- serde_json HashSet 序列化顺序不稳定——倒排比对用集合语义勿比数组序
