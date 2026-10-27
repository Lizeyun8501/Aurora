# DK-笔记属性 S1 派工单（标签/图标/题头图——core metadata 面）

**派给**: Bravo ｜ **派单**: Alpha 2026-10-27 22:40 ｜ **来源**: 用户裁决（思源对标属性区——「要做，请推进」）；R19 块插入面板姊妹卡（同一截图来源）

## 背景与设计基线

思源移动端编辑器属性区（添加标签/添加图标/添加题头图）→ Aurora 落地。**架构裁决（已拍板）**：

- **零 migration**：走 `Note.metadata: serde_json::Value` 现成字段——JSON 键域 `tags`/`icon`/`cover`，不动 entity schema、不动 SQLite
- **题头图走 AttachmentStore 现有体系**（sha256 内容寻址+attach_to_note 引用语义）——cover 值 = blob sha256 hex，GC/去重白得
- AGPL 红线同 R19：只对标交互语义
- **同步语义暂不扩**：metadata 随 note record 走现有 P2P 通道（不深改）；标签跨设备语义细化远期另卡

## 目标：`crates/aurora-core/src/note_meta.rs`（新模块，你的领地）

### API 面（签名定死，UI/S2 按此对接）

```rust
pub struct NoteMeta { pub tags: Vec<String>, pub icon: Option<String>, pub cover: Option<String> }
pub struct TagCount { pub tag: String, pub count: i64 }

pub async fn get_note_meta(ctx: &WriteContext, note_id: &str) -> Result<NoteMeta, Error>;

/// 全量覆写。校验失败返错（非法输入清单见下）。
pub async fn set_note_tags(ctx: &WriteContext, note_id: &str, tags: Vec<String>) -> Result<(), Error>;
pub async fn add_note_tag(ctx: &WriteContext, note_id: &str, tag: &str) -> Result<(), Error>;
pub async fn remove_note_tag(ctx: &WriteContext, note_id: &str, tag: &str) -> Result<(), Error>;
/// None = 清除。
pub async fn set_note_icon(ctx: &WriteContext, note_id: &str, icon: Option<&str>) -> Result<(), Error>;
/// None = 清除；Some(hash) 校验存在性。值 = attachment blob sha256 hex。
pub async fn set_note_cover(ctx: &WriteContext, note_id: &str, hash: Option<&str>) -> Result<(), Error>;

/// 全笔记聚合（含已删？不含——对齐 list_notes 的软删过滤惯例）。
pub async fn all_tags(ctx: &WriteContext) -> Result<Vec<TagCount>, Error>;
pub async fn notes_by_tag(ctx: &WriteContext, tag: &str) -> Result<Vec<NoteSummary>, Error>;
```

### 校验清单（fail-closed）

- tag：trim 非空；长度 ≤32 字符；单 note 总数 ≤16；**去重保序**（重复输入收敛首现位置）；控制字符拒绝
- icon：单 grapheme（emoji 多 codepoint 需正确判定——用现成 unicode 分段依赖，勿手写字节判断）
- cover：64 位 hex 小写；blob 必须已存在（AttachmentStore 读验）——不存在返 `Error`（错误码对齐 error_codes.rs 惯例，建议新增 `NoteMeta` 变体或复用最近域）
- note_id 不存在 → 标准 NotFound 语义

### 实现要点

1. **读-改-写合并**：`set_note_tags` 读 note record → 只改 metadata.tags 键 → 写回。**写回必须走 write_path 现有写编排**（串行化惯例——勿绕过写路径直写 KV 引入并发覆盖）
2. metadata JSON 键域 serde 默认值：旧记录无键 → 空 tags/None icon/None cover（**存量兼容零迁移**——serde default 模式对齐 write_path.rs `default_n()` 惯例）
3. `all_tags`/`notes_by_tag`：list_notes 内存聚合（个人量级 ≤10K 无压力；不做 SQL/索引优化，注释说明量级假设）
4. cover 校验调 `AttachmentStore`（`crates/aurora-core/src/attachment_store.rs`——blob_key/sha256_hex 现成）

## 边界（停手线）

- **UI/桥面（desktop/mobile/MainActivity JNI/uniffi）——S2 另卡**（本卡纯 core）
- **write_path.rs / app_core.rs / attachment_store.rs / links.rs——非你领地的核心编排文件勿动**；新命令的注册接入点若需碰 app_core，写成**独立 impl 块**+注记回执，Alpha 复核合入
- 搜索索引集成（标签进 tantivy）——S3 远期
- migration/entity——零改动（本卡设计基线，动了即违卡）

## DoD

1. 单测 ≥12 例全绿+既有全套零回归；clippy/fmt 零告警；CI 绿
   - 覆盖：全量覆写/去重保序/16 上限/32 长度界/控制字符拒/icon 单 grapheme（含多 codepoint emoji ✅+双字符 ❌）/cover 64hex 格式/b cover 不存在拒/note 不存在 NotFound/旧记录无键读回默认/聚合计数/软删过滤
2. 注记 issues/wf/bravo-DK-笔记属性S1.md（错误码选取/写路径接入点/坑）
3. push main 直推

## 已知坑

- **读-改-写竞争**：metadata 是单 JSON 字段——并发改 tags+icon 互相覆盖。必须走 write_path 串行编排（历史教训：双后台写链会产生锁竞争假死）
- serde_json Value 无 schema——读侧对非法类型（tags: "str" 而非数组）容错降级为空集+日志，勿 panic
- WriteContext 形状参考 `write_path.rs:338 set_note_encryption`（ctx+note_id+值三元组惯例）
- emoji grapheme 判定：项目依赖里找现成（unicode-segmentation 若已在树内直接用；不在树则挑最小件并在回执注明新增依赖理由）
