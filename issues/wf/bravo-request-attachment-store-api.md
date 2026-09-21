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

---

## Alpha 裁决（2026-09-20 20:5x · DK-09 S1+S2 集成时）

**结论：批准，排期 Alpha 冻结窗口（周一 9/21 起的独立切片）。**

- AttachmentStore trait 进 aurora-core（与 SyncTarget 同层）；KV 键设计（attach: 元数据 + attachblob: sha256 内容寻址去重）采纳。
- write_path 增 `attach_to_note` 唯一入口——纪律正确；**附件数据经 WriteContext.content_cipher 加密落库**（E2EE 盲区第 3 点的闭环方案，DK-07 栈复用）。
- ENEX 导入器改造点（written_to → attachment_id + aurora://attach/{id}）由 Bravo 在 API 落地后执行（< 1 人日，随 S3 或独立小切片）。
- `list_by_note` 级联清理语义：删除笔记 → 级联 delete 附件，写进 attach_to_note 的 WriteReceipt 副作用，禁止调用方手动清理。

---

## Alpha 落地回执（2026-09-21 · 冻结窗口切片完成）

**AttachmentStore API 已落地**（aurora-core `attachment_store` 模块 + write_path 入口）：

- `AttachmentStore` trait（async_trait，KVStore 同款风格）+ `KvAttachmentStore` 通用实现：
  `attach:{id}` 元数据 / `attachblob:{sha256}` 内容寻址去重 / `attachnote:{note}:{id}` 反向索引。
- `write_path::attach_to_note` / `read_attachment` 唯一入口；`delete_note` 自动级联清附件（失败不阻塞笔记删除，日志留痕）。
- `WriteContext.attachments: Option<Arc<dyn AttachmentStore>>`；bootstrap `BootedApp` 真实装配，desktop ATTACH_STATE 注入。

**两处技术修正（落地时确认，Bravo 导入器改造点按此执行）**：

1. **加密封装用 `WriteContext.seal`（vault DEK at-rest 字节封装），非 content_cipher**——后者是 `(note_id, &str)` 字符串接口，不适配二进制附件。附件与笔记正文（每笔记 HKDF）分层保护：at-rest 同 vault、字段级不套用。
2. **删除语义**：delete 只清 meta + 反向索引，**blob 不删**（内容寻址存储惯例：孤儿 blob 由 GC 切片回收，不做引用计数——避免"删 A 误伤 B 同内容附件"）。`read_attachment` 有明文 sha256 完整性校验（fail-closed），GC 前提下安全。

**导入器改造点（Bravo，<1 人日）**：`written_to: Option<PathBuf>` → 保留 sidecar 现状不动；新增 `attachment_id` 路径由 desktop command 层在导入后按需调用 `attach_to_note` 接管（正文链接 `aurora://attach/{id}` 改造随向导前端切片一并做，避免两改）。

**验证**：417 core（含 3 attachment）+ 34 import 测试全绿；bootstrap/import/mobile-ffi --all-targets 编译过；desktop import_commands 已注册（本机无 GTK，CI 桌面 job 验证）。
