# bravo-DK-双向链接S2 注记（Bravo 2026-10-16）

**commit**: 见 git log（feat(DK-双向链接S2)）｜ **前置**: 复核回执 f6bfff5（五项拍板）

## 拍板落实

| # | 拍板 | 落实 |
|---|---|---|
| 1 | from_index 变体 | `reindex_note_links_from_index(kv, note_id, new_content)`——旧集=KV links:out 权威读（缺省空集=首建）；old_content 参数版保留（测试场景）；diff 核心抽 `apply_link_diff` 两变体共享 |
| 2 | rename=B 悬挂惰性 | **零接入**（rename_note 未动——与 S1 语义自洽；A 链接跟随改写挂起池） |
| 3 | purge 原语 | `purge_note_links(kv, note_id)`——出链读→逐目标倒排 remove（空集删键）→删 out 键 |
| 4 | 回归面 | 两原语单测 2 例（from_index 权威旧集三向 diff+purge 级联 a/b 交叉断言）；core 全量 448 零回归 |
| 5 | 前缀登记 | Alpha 侧已落（f6bfff5 十域）——Bravo 无动作 |

## write_path 接线（save loro 版 + purge）

- **save_note_content**（loro-crdt cfg，blocks 派生后 3.5 位）：`record.encryption != ENC_AES256GCM` 才 reindex（与 blocks 同款锁定态纪律——图谱/反链面不消费锁定篇）+失败非阻断 warn（notes/快照权威，索引滞后后续保存补齐）
- **purge_note**（附件级联后）：purge_note_links 非阻断（附件级联先例同款——最终一致，残余悬挂 S1 惰性兜底）
- **无 Loro 降级版 save**：零接入（与 blocks 降级语义一致——注记披露）；rename/delete_note：零接入（拍板 2+语义自洽）
- **create_note**：零接入（空内容无链）

## 验证矩阵

| 门 | 结果 |
|---|---|
| links 单测 | 17 ok（15+2） |
| core 全量 | 448 ok 零回归 |
| clippy（CI 同款 --all-targets 全参） | 0 |
| fmt --check | 0 |
| **loro-crdt cfg 编译** | `cargo check -p aurora-core --features loro-crdt` =0（save 接线类型/语法实证——本地无 GTK 之外的 cfg 面自验） |

## 诚实边界

- write_path 行为级 links 断言（save/purge 全链）未补——write_path 测试需 WriteContext+loro 运行时面，本地 default 面不可达；原语级行为断言已覆盖（17 例），S2 命令层 e2e 补
- 倒排一致性：from_index 假设 KV 旧集与实际内容一致（无 reindex 钩子的历史路径可能漂移——首保存即收敛）
- rename=A 选项挂起池（回执原文）——需另单
