# Bravo Request：附件存储 API（DK-09 S2 ENEX 导入阻塞项）

> 发起：Bravo · 2026-09-20 · 收件：Alpha
> 依据：DK-09 任务书 §7-1「资源先落附件区——附件 API 若缺，写 request」

## 1. 背景

ENEX 导入（S2 已交付）解析出 `<resource>`（base64 编码的图片/附件，含
mime + 原始文件名 + MD5 hash）。ENML 里的 `<en-media hash=...>` 引用需
要落地为真实附件。当前 aurora-core **没有任何附件存储/关联 API**。

S2 的过渡方案（已实现）：`EnexImportOptions.attachments_dir: Option<PathBuf>`
— 资源解码后写到调用方指定的 sidecar 目录（`<hash12>-<basename>`），
笔记正文以 `![name](attachments/<file>)` 相对链接引用；未提供目录时
输出 `attachment://<hash>` 占位 + warning。

## 2. 缺口

1. 笔记与附件无关联（删除笔记遗留孤儿文件；检索/同步不含附件）。
2. sidecar 相对链接依赖「笔记存储位置/导出方式」，桌面 webview 渲染
   时 base 路径未定义。
3. E2EE 体系下附件无加密语义（DK-07 加密落库不覆盖文件）。
4. 同步层（aurora-sync）无法增量传输附件。

## 3. 提议 API（aurora-core 新 trait，M3 前落地即可）

```rust
pub trait AttachmentStore: Send + Sync {
    /// 写入附件（内容寻址：sha256(key)），返回 attachment_id。
    async fn put(&self, note_id: &str, file_name: &str,
                 mime: &str, data: &[u8]) -> Result<String, Error>;
    /// 读取（校验完整性）。
    async fn get(&self, attachment_id: &str) -> Result<Option<Attachment>, Error>;
    /// 笔记的全部附件（删除笔记时级联清理）。
    async fn list_by_note(&self, note_id: &str) -> Result<Vec<AttachmentMeta>, Error>;
    async fn delete(&self, attachment_id: &str) -> Result<(), Error>;
}
```

- KV 键：`attach:{id}` → 元数据（note_id/file_name/mime/size/sha256），
  数据本体 `attachblob:{sha256}`（内容寻址天然去重）。
- write_path 增 `save_note_content` 同级的 `attach_to_note` 写入口，
  保持「唯一入口」纪律。
- ENEX 导入器改造点：`ResourceInfo.written_to: Option<PathBuf>` →
  `attachment_id: Option<String>`，正文链接改为 `aurora://attach/{id}`，
  渲染层经 FFI/command 解析。

## 4. 建议

- 批准后由 Alpha 排期到 DK-09 S3/M3 之间任意切片（导入器改造点 < 1 人日）。
- S2 交付不受阻塞：sidecar + 占位方案已测试覆盖，API 落地后平滑切换。
