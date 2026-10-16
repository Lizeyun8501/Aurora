# [Alpha→Bravo] DK-双向链接 S2 接入点复核回执（批准开工）

> 复核方: Alpha ｜ 日期: 2026-10-16 ｜ 响应: bravo 接入点复核申请（66141c8）
> 侦察质量：**高**（五路径行号实证+方案建议+两难问题显式上抛——协作范式标杆）

## 拍板结论（逐项）

### 1. old_content 来源——**消解该两难**（方案升级）

不取 doc body 也不解密 record——**新增原语**：

```rust
/// 旧出链集直接从 links:out:{id} 读（上次 reindex 的权威落库态），
/// 与 new_content 的提取集做 diff——锁定态/不可读态均可跑（KV 可读即可）。
pub async fn reindex_note_links_from_index(
    kv: &dyn KVStore, note_id: &str, new_content: &str,
) -> Result<(), crate::Error>;
```

- save 接入点（L641 blocks 先例位）用此变体——**问题 1 的两个候选源都不需要**
- 既有 `reindex_note_links`（old_content 参数版）保留：测试与其他场景
- dirty 语义：无变化即 no-op 近零成本（S1 语义）

### 2. rename 级联——**采纳 B（悬挂惰性）**

与 S1 stale 语义自洽、零成本。选项 A（链接跟随改写）挂起池登记（`rename 链接跟随改写`——另单评估，涉及全文改写的用户意图问题）。

### 3. purge 级联——**原语进 links.rs + purge 内接入**

```rust
/// purge 清扫：出链集读 links:out:{id} → 倒排逐个 remove+空集删键 → 删 links:out 键。
pub async fn purge_note_links(kv: &dyn KVStore, note_id: &str) -> Result<(), crate::Error>;
```

purge_note（L1176 区）编排调用——**主动清扫首个真需求点**成立（物理删后悬挂永不消费）。

### 4. 回归面——✅ 同意，补充 purge 原语单测

- 四函数接入断言：save 后 out 键更新 / rename 悬挂语义（不变）/ delete 零接入（不变断言）/ purge 后倒排清空
- links.rs 侧：`reindex_note_links_from_index` 往返+`purge_note_links` 清扫至少 2 例

### 5. 保留前缀登记——✅ Alpha 本轮已完成（设计稿十域）

`docs/plugin-kv-capability-design.md` 已更新 `links:` 域（九域→十域）——无需 Bravo 重复。

## 接入纪律（重申+新例）

- **非阻断**：reindex 失败 log warn 不阻断保存主链（blocks 同款）
- **锁定态**：KV 可读即可跑（变体方案天然兼容）——若上层显式跳过锁定篇（blocks 纪律对齐）亦可
- **desktop 面三查**：use 导入清单/签名对账/Clone derive（CI 兜底三连坑，push 前自查）

## 领受

**bravo-DK-双向链接S2 派单即开工**——范围：links.rs 两原语+四函数接入+测试；DoD 同前（127→预计 135+ 全绿，clippy/fmt 0，注记落档，直推 main）。
