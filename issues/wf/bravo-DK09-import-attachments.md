# Bravo 任务书：导入器附件接管（DK-09 收官切片）

> 发起：Alpha · 2026-09-22
> 接收：Bravo（领地 **crates/aurora-import/**\*\*）
> 依赖：AttachmentStore API（d4bbe22）+ `write_path::attach_to_note`（唯一入口）+ desktop `ATTACH_STATE` 注入（ae75ab1 前置）——**全部已就绪**
> 性质：DK-09 最后一块。改造量 <1 人日（request 裁决原文），实际含测试约 1.5 人日。

## 1. 背景与现状

五格式导入（markdown / enex / opml / html / notion）已交付，正文写入严格走
`write_path::create_note + save_note_content` 唯一入口。**附件是唯一缺口**：

- `enex.rs`：`RawResource`（base64 data + mime + file-name + hash）**已解析、未落库**——
  解析后被丢弃，正文里 `<en-media>` 无对应引用；
- `markdown.rs` / `notion.rs`：导出目录内的图片/资源文件未处理，链接原样保留
  （导入后指向不存在路径）；
- `report.rs`：`ImportReport` 无附件维度计数。

`write_path::attach_to_note(ctx, note_id, file_name, mime, data) -> AttachmentMeta`
已提供：seal 字节封装（桌面 vault / 移动明文降级）+ sha256 内容寻址 blob 去重 +
笔记存在性校验，**不触碰正文版本**（updated_at/blocks/事件流不动）。desktop
command 层 `ctx.attachments` 已注入，向导调用即生效。

## 2. 附件引用形态（Alpha 定调，无需 request）

markdown 正文中统一用 **`attachment://{attachment_id}` URI**：

- 图片类（mime 前缀 `image/`）：`![file-name](attachment://{id})`
- 其他资源：`[file-name](attachment://{id})`

> 渲染侧（desktop 前端解析该 scheme → `read_attachment`）属 Alpha/desktop
> 领地，**后续切片跟进，本次不阻塞**——引用先正确落库。

## 3. 任务清单

### 3.1 enex 资源落库（P0）

1. `import_one`（enex 路径）：对每个 `RawResource` 解 base64 →
   `attach_to_note(ctx, &note_id, file_name, mime, &data)`；
2. ENML → markdown 转换（`enml.rs::enml_to_markdown`）时 `<en-media hash=X>`
   → 依 **hash → attachment_id 映射** 重写为 §2 引用形态
   （注意：Evernote 的 `hash` 是其自身摘要，与我们的 sha256 id 不同，须查表重写）；
3. 资源 attach 失败 → 该笔记失败（fail-closed，不产出半截引用）。

### 3.2 markdown / notion 资源文件（P1）

1. markdown：正文相对路径图片/资源 → 从 `source_dir` 解析真实文件 →
   attach → 链接重写为 `attachment://{id}`（文件缺失：保留原链接 +
   report 计 `attachments_missing`，不失败——宽松语义，与 ENEX 的严格语义区分）；
2. notion：同 markdown 处理 `resources` 列表（`notion.rs:133` 现为 `Vec::new()` 占位）。

### 3.3 report 计数（P1）

`ImportReport` 增 `attachments_imported: u64`；markdown 缺失路径增
`attachments_missing: u64`。

### 3.4 已知 request 点（P2，阻塞 report 精确性但不阻塞主体）

`KvAttachmentStore::put` 现返回 `()`——blob 去重内部静默，Bravo 无法计
`deduped` 数。如需精确去重计数，**写 request**（`put` 返回 `Result<bool>`，
existed 标志；core 侧影响面小，Alpha 快批）。任务书不预批——按流程走。

## 4. 边界约束

- **领地**：`crates/aurora-import/**`；**禁改** aurora-core（§3.4 例外走 request）、
  desktop command 层、向导前端；
- 附件写入**只经 `attach_to_note`**，禁止旁路 `store.put` 或直接落 KV；
- 不得改动正文写入路径（`create_note`/`save_note_content` 调用点签名）；
- 测试资源文件放 `crates/aurora-import/tests/fixtures/`（如无则新建），
  单文件 <10KB（防仓库膨胀）。

## 5. 验收标准（全绿才算切片完成）

- [ ] `cargo +1.91.0 test -p aurora-import` 全绿 + clippy 零 warning + fmt 清；
- [ ] enex 集成测试：含 2 资源笔记导入 → `list_by_note` 返回 2 条 meta，
      正文含 2 处 `attachment://` 引用，hash→id 重写正确；
- [ ] 去重语义测试：同资源挂两笔记 → blob 只存一份（kv blob 键计数断言）；
- [ ] `read_attachment` 回读字节与源文件一致（seal/明文两模式）；
- [ ] markdown 缺失资源 → 计入 `attachments_missing` 且导入不失败；
- [ ] 分支 `wf/bravo-import-attach`，commit 规范，**交付前
      `git fetch && git log origin/main..HEAD` 自查**（防旧分支重推）。

## 6. 流程约定

改动超边界 / 接口缺口 → request 文档（`issues/wf/bravo-request-*.md`），
不硬改。交付后 Alpha 验收回执写回本文档。


---

## Alpha 验收回执（2026-09-23 · commit fec3d98）

**✅ 通过合入 main（ff）。**

- [x] 测试全绿：lib 5 + attach_import 6 + doc 8 = 19 tests，T=0；
      clippy 零 warning（C=0）；fmt 清（F=0）。
- [x] enex 集成：`dk09_attach_enex_two_resources`——2 资源 attach +
      `list_by_note` 断言 + 正文 `attachment://` 引用 + hash→id 查表重写 ✓
- [x] 去重语义：`dk09_attach_dedup_blob_single_copy`（kv blob 键计数）✓
- [x] 回读一致性：`dk09_attach_readback_plaintext_and_sealed`（seal/明文双模式）✓
- [x] markdown 缺失容忍：`dk09_attach_md_missing_resource_tolerant` ✓
- [x] 边界合规：改动全部落在 `aurora-import/**` + Cargo.lock + request 文档；
      分支 `wf/bravo-import-attach` 干净单 commit。
- 加分项：`dk09_attach_fail_closed_no_residual`（FailingStore 注入故障 →
  fail-closed 无残留引用，超出任务书要求）；`dk09_attach_md_cache_reuse`
  （同路径复用 attachment id）；无附件能力 fallback（`attachment://<hash>`
  占位 + warning，向后兼容旧调用方）。
- 遗留：request（put existed 标志）已批复**批准**（见该文档 Alpha 裁决段），
  Bravo 随补丁交付 `attachments_deduped` 计数 + FailingStore 对齐；
  渲染侧 `attachment://` scheme 解析（desktop）转 Alpha 切片。
