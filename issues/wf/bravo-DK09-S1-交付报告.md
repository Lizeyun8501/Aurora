# Bravo 交付报告：DK-09 — S1 Markdown 目录导入 + S2 ENEX 导入

> 执行：Bravo · 2026-09-20 · 分支 `wf/bravo-import`（基于 main 70438e8）
> S1：0.5 人日 · S2：0.5 人日

## 1. 交付物

| 文件 | 说明 |
|---|---|
| `crates/aurora-import/`（新建 crate） | 导入器本体：`lib.rs`（目录扫描+编排）、`markdown.rs`（frontmatter 剥离+标题提取）、`report.rs`（报告/错误） |
| `crates/aurora-import/tests/markdown_import.rs` | 8 个 `dk09_*` 集成测试（真实 AppCore 装配） |
| 根 `Cargo.toml` | **仅一行** members 追加 `"crates/aurora-import",` |
| `Cargo.lock` | 新 crate 依赖登记（自动生成） |

## 2. API 一览

```rust
// 入口（lib.rs）
pub async fn import_markdown_dir(
    ctx: &WriteContext,          // 调用方装配（与 write_path 同款依赖注入）
    dir: &Path,
    options: &ImportOptions,     // include_hidden: bool = false, max_depth: usize = ∞
) -> Result<ImportReport, ImportError>;

pub struct ImportReport {
    pub scanned: usize,          // .md 候选数
    pub imported: usize,         // 成功数
    pub skipped: usize,          // 非 .md / 隐藏文件
    pub failed: usize,           // 失败数（守恒：scanned = imported + failed）
    pub note_ids: Vec<String>,   // 创建顺序的笔记 ID
    pub errors: Vec<ImportError>,// 逐文件失败明细（path + reason）
    pub warnings: Vec<String>,   // frontmatter 无 title / 未闭合等
    pub duration_ms: u128,
}

// markdown.rs（可独立复用/单测）
pub fn split_frontmatter(content: &str) -> (Option<&str>, &str); // 未闭合 → 原文
pub fn extract_title(frontmatter: &str) -> Option<String>;       // 仅顶层 title:，去引号
pub fn md_to_note(content: &str, stem: &str) -> (String, String, Vec<String>);
```

## 3. 关键设计决策

1. **写入严格走 write_path 唯一入口**（`create_note` + `save_note_content`），
   与桌面/移动端同源，blocks 派生与事件投影自动生效，不旁路落盘。
2. **内容无损（§1 道德底线）**：body 用 `split_inclusive('\n')` 切片，
   逐字节保留行尾（CRLF/LF）；唯一变换 = frontmatter 块剥离 + 标题确定。
   frontmatter 解析失败（未闭合）→ **原样导入 + warning**，fail-open 到无损方向。
3. **标题规则**：frontmatter 顶层 `title:`（去引号）优先，否则文件名 stem；
   frontmatter 无 title → stem + warning（任务书 §5.3-2）。
4. **隐藏文件默认跳过**：相对路径任一分量 `.` 开头即跳（防 `.obsidian/` 误入）。
5. **自写最小 YAML 标量解析**（仅顶层 `key: value`），不引入 serde_yaml。
6. **`.md` 与 `.markdown` 双扩展名**均视为候选。

## 4. 测试（8 个，全部真实执行 — S5 教训已过 grep 核验）

| 测试 | 覆盖 |
|---|---|
| `dk09_import_markdown_dir_basic` | 基础导入 + 正文逐字符读回（含代码块）|
| `dk09_import_nested_subdirectories` | 多层子目录递归（a/b/c.md）|
| `dk09_import_title_from_frontmatter` | frontmatter title 优先于 stem；无 frontmatter 用 stem |
| `dk09_import_frontmatter_stripped` | frontmatter 剥离后正文不含 `---`/title 行 |
| `dk09_import_frontmatter_fallback_warns` | 无 title / 未闭合 → stem + warning + 正文原样 |
| `dk09_import_skip_non_markdown_and_hidden` | .txt/.png/隐藏文件跳过 + include_hidden 开关 |
| `dk09_import_no_loss_complex_markdown` | **底线测试**：代码块/表格/任务列表/引用逐字符读回 |
| `dk09_import_report_counts_and_empty_dir` | 计数守恒、错误明细、空目录、无效 UTF-8 → failed |

测试装配：`aurora_bootstrap::bootstrap(tempdir)`（与桌面端同源 SQLite KV +
blocks + 启动流程）；读回断言走 `load_note_meta` → `record.content`
（`get_note_content` 路径在 core 侧的权威等价物，加密级 none 即明文）。

## 5. 验证矩阵

| 门槛 | 结果 |
|---|---|
| `cargo +1.91.0 test -p aurora-import` | **13/13 全绿**（8 集成 + 4 单元 + 1 doc）|
| `cargo +1.91.0 clippy -p aurora-import --all-targets -- -D warnings` | **0 warning** |
| `cargo +1.91.0 fmt --all -- --check` | 干净 |
| 根 Cargo.toml diff | **仅一行** members 追加 |
| 测试名单 grep 核验（任务书 §8-4） | 8 个 `dk09_*` 逐一确认 `... ok` |

## 6. 偏差与备注

1. **仓库路径**：任务书写 `/home/z/my-project/repos/Aurora`，实际克隆在
   `/home/z/my-project/Aurora`（同一远端），分支已从 origin/main(70438e8) 切出。
2. **walkdir 顺序**：`sort_by_file_name` 按文件名字节序（非 locale 序），
   测试断言已做成顺序无关；报告 `note_ids` 顺序 = 遍历顺序，文档已注明。
3. **Read 工具属主坑（§8-2）**：Write 工具落盘 root 属主导致 fmt 拒写，
   已用重写文件方式修复为本用户属主（后续 Write 落盘的文件需注意）。
4. **default-members 未加**：保持 diff 一行约束；CI 若用 `--workspace`
   或显式 `-p aurora-import` 均可覆盖（members 已含）。如 Alpha 希望
   默认编译集也含 import，请示下后补一行。
5. **重复文件名**：同名 stem 导入为多篇独立笔记（uuid id），不合并不去重
   （M3 防重/选择性导入再处理，与任务书 §7-3 对齐）。

## 7. S2 追加交付 — ENEX 解析导入（同分支）

| 文件 | 说明 |
|---|---|
| `src/enex.rs` | ENEX 流式解析（quick-xml 0.41，CDATA/实体引用/多 note/resource） |
| `src/enml.rs` | ENML→Markdown 转换（标题/列表/任务/代码/表格/链接/en-media/en-crypt） |
| `src/lib.rs` | `import_enex(ctx, path, &EnexImportOptions)` 编排 + 资源 sidecar |
| `tests/enex_import.rs` | 8 个 `dk09_enex_*` 测试（资源 base64 往返逐字节断言） |
| `issues/wf/bravo-request-attachment-store-api.md` | **request：附件存储 API 提议**（S2 阻塞项，任务书 §7-1 指令） |

要点：
- 资源过渡方案：`attachments_dir: Option<PathBuf>` sidecar 落盘
  （`<hash12>-<basename>`），正文 `![名](attachments/<落盘名>)`；
  未提供目录 → `attachment://<hash>` 占位 + warning。
- ENML 有损映射原则：文本逐字符保留；结构标签尽力映射；未映射标签
  透传 + warning；en-crypt 不解密输出占位（后续切片处理）。
- quick-xml 0.41 注意：`Event::GeneralRef` 为实体引用独立事件
  （`&amp;` 不再走 Text），已实现预定义实体+数字字符引用解析。

## 8. S2 验证矩阵

| 门槛 | 结果 |
|---|---|
| `test -p aurora-import` | **21/21 全绿**（8 enex + 8 markdown + 4 单元 + 1 doc）|
| `clippy --all-targets -D warnings` | **0 warning** |
| `fmt --all -- --check` | 干净 |
| 测试名单 grep 核验 | 16 个 `dk09_*` 逐一确认 `ok`（S5 教训）|

## 9. 下一切片建议（§7 队列）

1. 附件存储 API（见 request，落地后导入器改造 < 1 人日）
2. ENEX 导入器 CLI/桌面接线（desktop 调用点——先 request）

---

## Alpha 验收记录（2026-09-20 20:5x · 合入 main）

**S1 + S2 验收通过。**

- 本地独立复核：21/21 全绿（8 enex + 8 markdown + 4 单元 + 1 doc）+ fmt 干净 + clippy 过（&& 链证明：workspace check 段启动 ⇒ 前三段退出 0；check 的 gdk-sys 失败系本机无 GTK dev 库，非本卡范围，CI 桌面 job 不受影响）
- 根 Cargo.toml diff 仅一行（default-members 刻意不加，成员一致性由 --workspace 校验——采纳）
- 交付纪律：S5 教训执行到位（测试名单 grep 核验）、request 提前量正确（S2 不阻塞）
- 附件 API 裁决见 request 文档；S3 待 Alpha 落 API 后启动
