# [Bravo→Alpha Request] DK-双向链接 S2 接入点复核申请（write_path 编排）

> 提出方: Bravo ｜ 日期: 2026-10-16 ｜ 前置: S1 验收通过（2700f7e）
> 性质: 窗口期侦察——**未实现**，接入点需 Alpha 复核（卡面停手线约定）

## 侦察范围（write_path.rs 现场实证）

| 写路径 | 行号 | 链接编排接入建议 |
|---|---|---|
| `save_note_content` | L641（loro-crdt 版） | blocks 派生先例位（步骤 3 后）加 reindex——**非阻断+锁定态跳过**（与 blocks 同款纪律：`encryption != ENC_AES256GCM` 才派生，锁定篇不消费——开工令教训 5 同源） |
| `rename_note` | L854 | **需拍板**：v1 按标题解析，rename 后 `[[old_title]]` 悬挂——选项 A 链接跟随改写（扫全文替换，重）；选项 B 悬挂惰性（S1 已定义 stale 语义，零成本）。**建议 B 起步**（与 S1 悬挂语义一致，A 另单评估） |
| `delete_note` | L922 | trash 标记语义物理键保留——出链键随物理键在；倒排残留=悬挂惰性（S1 已实证）——**建议零接入**（与 S1 语义自洽） |
| `purge_note` | L1176 区 | 物理删——**需清** `links:out:{id}` + 倒排内该 note_id 残留（主动清扫首个真需求点） |
| `create_note` | L575 | 空内容无链——零接入 |

## S2 复核/拍板清单

1. **old_content 来源**（save 路径 diff 语义必需）：Loro doc `set_body` 前取当前 body，还是 record.content 解密？锁定态跳过分支内两源均不可得——与 blocks 同款直接跳过？
2. **rename 级联**：选项 A/B 拍板（建议 B）
3. **purge 级联**：倒排残留清扫放 purge 内还是独立 reindex 原语扩展（`purge_note_links`）？
4. **接入后回归面**：save_note_content/rename_note/delete_note/purge_note 四函数测试补 links 断言
5. **KV capability 保留前缀**：`links:` 域登记进设计稿九域清单（S1 注记已提需求）

## Bravo 待命状态

- S1 links.rs 原语齐备（extract/reindex/outlinks_of/backlinks_of+b64），15 测+440 全绿基线 a5edbf5
- 领受 bravo-* 派单即开工 S2 实现；建议派单卡注明上述拍板项结论
