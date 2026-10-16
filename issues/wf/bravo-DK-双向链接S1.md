# bravo-DK-双向链接S1 注记（Bravo 2026-10-16）

**commit**: 见 git log（feat(DK-双向链接S1)）｜ **派单**: 0f60620 ｜ **来源**: 审阅 P0-1

## 语法边界决策（v1）

| 面 | 决策 |
|---|---|
| 提取 | [[target]] / [[target|alias]]——target=竖线前段 trim |
| 代码块 | 三反引号围栏（跨行状态机，围栏行自身不参与）内忽略 |
| 行内码 | 成对单反引号段内忽略（未闭合行内码=整行后半段忽略——保守） |
| 空目标 | [[]]/[[ ]]/[[|alias]] 跳过 |
| 去重 | 保序（首现顺序）——HashSet 语义在倒排侧，出链保留序 |
| 未闭合 [[ | 忽略（不截行） |
| 解析 | v1 按标题；[[note-id]] 形态 S2 随 UI 语法 |

## 键域形状（登记需求：KV capability 保留前缀清单需补 links: 域）

- `links:out:{src_note_id}` = serde_json(Vec<String>)——**全量覆写**（删旧写新，空集也写=幂等清晰）
- `links:in:{target_title_b64}` = serde_json(HashSet<note_id>)——**增量维护**（读旧集-apply diff-写回；**空集删键**防装卸膨胀）
- b64=urlsafe no-pad **手写实现**（core 无 base64 依赖不引新 crate；中文/斜杠安全；decode 非法=键读不到=空集 NotFound 语义）

## 悬挂边语义（v1 惰性）

- 目标删除/改名**不主动清扫**（S2 决策清扫策略）——倒排残留 note_id
- 查询侧（S2）按 existence 过滤，悬挂 stale 不展示；本模块保证 diff 语义精确
- 测试实证：直接清 out 键（模拟无钩子删除）→倒排残留保留

## 维护语义

- `reindex_note_links(kv, note_id, old, new)`——出链 diff：added 进倒排/removed 出倒排；rename 由调用方（S2）以旧内容/重定向后内容编排
- `outlinks_of`/`backlinks_of` 读原语（S2 命令层数据源）
- write_path/sync 集成未做（S2，需 Alpha 复核接入点）

## 验证矩阵

| 门 | 结果 |
|---|---|
| links 单测 | 15 ok（12 用例+3 辅助） |
| aurora-core 全量 | 440 ok 零回归 |
| clippy（CI 同款 --all-targets 全参） | 0 |
| fmt --check | 0 |

## 诚实边界

- 图谱/反链 UI（S3）、命令层编排（S2）、id 解析（S2）未做
- e2e 未加（卡面不强求——core 面单测完备；S2 命令层接 e2e）
- 倒排比对集合语义（serde_json HashSet 序列化序不稳定——测试不比数组序）
- links.rs 未导出 re-export（lib.rs pub mod 即可；S2 按需补）
