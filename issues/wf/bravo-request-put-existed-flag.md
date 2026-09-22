# Bravo Request：KvAttachmentStore::put 返回 blob 已存在标志（existed）

> 发起：Bravo · 2026-09-22
> 接收：Alpha（aurora-core 领地）
> 依据：任务书 bravo-DK09-import-attachments §3.4 —— put 现返回 `()`，
> blob 去重内部静默，导入器无法计 `deduped` 数。任务书明确「不预批——按
> 流程走」，本文档即该流程。
> 状态：**待 Alpha 裁决**。当前切片已按 §3.4 备选口径交付
> （`attachments_imported` 计 attach 调用成功次数，同 blob 重复 attach 各计
> 一次），本 request 不阻塞交付。

## 1. 请求变更

```rust
// aurora-core/src/attachment_store.rs · AttachmentStore trait
-    async fn put(&self, meta: &AttachmentMeta, sealed_data: &[u8]) -> Result<(), Error>;
+    /// 返回 blob 是否已存在（true = 内容寻址命中，复用既有 blob，未写新数据）。
+    async fn put(&self, meta: &AttachmentMeta, sealed_data: &[u8]) -> Result<bool, Error>;
```

KvAttachmentStore 实现：`blob_exits(sha256)` 命中时返回 `Ok(true)` 且不再
写 KV；未命中写入后返回 `Ok(false)`。唯一实现是 KvAttachmentStore
（attachment_store.rs 单一实现，无其他 mock 需改——desktop/移动端均经
`write_path::attach_to_note` 调用，不直触 store）。

## 2. 动机

任务书 §5 去重语义验收只要求 blob 键计数（已由测试
`dk09_attach_dedup_blob_single_copy` 覆盖：scan_prefix 断言 blob 只存一份，
**已满足，不依赖本变更**）。但向导前端报告页若要展示「复用 N 条附件」
的精确数字，只能由 put 的 existed 标志回传——现在导入器侧无感知通道。

## 3. 影响面

| 位置 | 影响 |
|---|---|
| `aurora-core/src/attachment_store.rs` | trait 签名 1 处 + KvAttachmentStore 实现 1 处（内部已查 blob_exits，改为携带结果返回，几乎零增量） |
| `aurora-import`（Bravo 领地） | `ImportReport + attachments_deduped: usize` 计数 + enex 附件模式累计；前端向导报告页加一行展示 |
| desktop / mobile-ffi / 其他 | **零影响**（不直触 store；trait 改签名后编译期强制对齐，当前唯一直接调用点 write_path::attach_to_note 只透传） |

## 4. 风险评估

低：put 语义不变（成功仍成功），仅返回值从 () 变 bool；失败路径不变
（本切片 fail-closed 行为不受影响——existed=true 时 attach_to_note 仍写
meta、仍返回新 attachment_id，去重只在 blob 层）。

## 5. Bravo 侧就绪代码

不预写——待裁决后 0.5 人日内随补丁交付（含测试：同资源二次 attach →
existed=true → report.attachments_deduped 计数断言）。
