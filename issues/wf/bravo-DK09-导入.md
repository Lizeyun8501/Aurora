# Bravo 任务书：DK-09 第一切片 — Markdown 目录导入（M2）

> 发起：Alpha · 2026-09-20（DK-08 验收通过后分派）
> 执行：Bravo
> 预算：第一切片 3–5 人日
> 依据：v26_issue清单 [DK-09] + [RV-03]（导入器前置：ENEX/Markdown 目录随 M2 交付，其余 M3）
> 前置：DK-08 已全部合入 main（6558187），你的 aurora-sync 领地工作已验收归档

## 1. 你的使命

让「从旧软件无痛迁移」的第一入口真实发生：**用户把一个 Markdown 目录（含子目录）交给 Aurora，目录里每个 .md 文件变成一篇笔记，块结构无损**。这是 DK-09（导入导出与迁移向导，30 人日卡）的第一切片，为 ENEX 导入（第二切片）打基建。

**道德底线**（清单原文）：导出 Markdown 零障碍、永不设限——导入侧同理：**Markdown 导入不得有格式损失**（代码块/表格/任务列表原样保留，因为我们存的就是 Markdown 体）。

## 2. 环境速查

- 仓库：`/home/z/my-project/repos/Aurora`（先 `git fetch` 基于最新 main 开分支 `wf/bravo-import`）
- 工具链：`cargo +1.91.0`（**不要用** /home/z/.local/rust——已退役；cargo 在 /home/z/.cargo/bin）
- 环境变量：`export RUSTUP_HOME=/home/z/.rustup CARGO_HOME=/home/z/.cargo CARGO_INCREMENTAL=0`
- 测试：`cargo +1.91.0 test -p aurora-import`
- 磁盘治理（重要，当前 90%）：编译前清一次 `rm -rf target/debug/{deps,incremental}`；不要跑全量 `cargo clean`

## 3. 领地（可修改）与禁改

**可修改（你的新领地）**：
- `crates/aurora-import/**`（新建 crate，本卡核心交付物）
- 根 `Cargo.toml` 的 `members` 数组：**只允许追加一行** `"crates/aurora-import"`（这是共享文件，不得改动其他行；CI 会盯 diff）

**禁改（只读参照）**：
- `crates/aurora-core/**`（尤其 write_path.rs——通过公开 API 调用，缺接口写 request 文档）
- `crates/aurora-sync/**`、`apps/**`、CI 配置
- 其他成员 crate 一律不碰

**接口缺口处理**：与 DK-08 同规约——临时方案交付 + `issues/wf/bravo-request-*.md` 写清缺口、临时方案、申请方案、影响面、验收清单，Alpha 裁决。

## 4. 现状盘点（Alpha 已核实，直接用）

### 4.1 写入入口（aurora-core::write_path，全部公开 API）

```rust
// 建笔记：返回 note_id
pub async fn create_note(ctx: &WriteContext, title: &str) -> Result<String, Error>;
// 写内容：Markdown 文本直接进 Loro doc（doc.set_body(content, now_ms)）
pub async fn save_note_content(
    ctx: &WriteContext, note_id: &str, content: &str,
) -> Result<WriteReceipt, Error>;
```

- `WriteContext` 构造参照 `crates/mobile-ffi/src/lib.rs:341`（含 content_cipher 字段——DK-07 S4；导入路径传 `seal: None` + 默认 cipher）
- **不要绕过 write_path 直写 core/blocks**——加密、Mirror 事件级落盘、审计都在这条路上

### 4.2 块模型

笔记体是单个 Markdown 文本（Loro doc body）。导入 = 原文 `save_note_content`，**不需要**逐块解析——除非文件带 frontmatter（见 5.1）。

### 4.3 测试样板

`crates/aurora-sync/src/external/webdav.rs` 的 `mod tests`（mockito + tempdir 模式）；`conflict.rs` 测试风格（fail-closed 断言）。

## 5. 第一切片任务：Markdown 目录导入

### 5.1 新建 `crates/aurora-import/`

```
crates/aurora-import/
├── Cargo.toml          # 依赖：aurora-core(path), tokio, walkdir, 轻量 frontmatter 解析（自写或 minimal 少依赖）
└── src/
    ├── lib.rs          # pub mod markdown; pub mod report;
    ├── markdown.rs     # 目录扫描 + frontmatter 剥离 + write_path 写入
    └── report.rs       # ImportReport { total, ok, skipped, failed, failures: Vec<(path, reason)> }
```

核心函数签名（建议，可微调）：

```rust
pub async fn import_markdown_dir(
    ctx: &WriteContext,
    dir: &Path,
    opts: &ImportOptions,       // max_files(默认 5000)、单文件上限（默认 2MB，防误导入巨型日志）
) -> Result<ImportReport, Error>;
```

### 5.2 行为要求

- **递归扫描** `dir` 下所有 `*.md`（walkdir）；跳过隐藏目录/`.git`/`node_modules`/`target`
- **frontmatter 处理**：文件头 YAML frontmatter（`---` 围栏）剥出 `title`（缺省用文件名去扩展名），其余 frontmatter **丢弃但记入 report**（M3 向导再利用）
- **标题冲突**：同 title 多文件 → 各自独立笔记（不合并），report 标注
- **fail-per-file**：单文件失败不中断整批，失败项进 `failures`（路径+原因）；整体返回 report
- **幂等性标记**：不做去重（同目录导两次 = 两份笔记）——M3 向导加防重；report 里如实反映
- 顺序写入（不并发——write_path 链路未审计并发安全，别开先河）

### 5.3 测试（markdown.rs 内 mod tests，tempdir 造样例目录）

至少覆盖（全部要真实断言，不许 `assert!(true)`）：

1. `dk09_import_flat_dir_all_created`——平级 3 文件全导入，report.total==3 && ok==3，且逐篇 `save_note_content` 读回内容一致（用 get_note_content 路径读回验证）
2. `dk09_import_nested_dirs_recursed`——子目录递归命中
3. `dk09_import_frontmatter_title_extracted`——frontmatter title 优先生效；无 frontmatter 用文件名
4. `dk09_import_bad_file_fails_per_file`——一个不可读文件（权限或非 UTF-8）不中断，failures 记录正确
5. `dk09_import_oversize_file_skipped`——超 max 单文件上限跳过并记录
6. `dk09_import_codeblock_content_lossless`——含代码块/表格/任务列表的文件读回逐字符一致（道德底线测试）

### 5.4 交付物

- crate 代码 + 测试全绿
- `issues/wf/bravo-DK09-切片1-交付报告.md`：切片摘要、测试清单、report 结构说明、遇阻与 request 文档链接（若有）

## 6. 验收标准（全绿才算切片完成）

- [ ] `cargo +1.91.0 test -p aurora-import` 全绿（默认 feature）
- [ ] `cargo +1.91.0 clippy -p aurora-import --all-targets` **零 warning**
- [ ] `cargo +1.91.0 fmt --all --check` 干净
- [ ] 根 Cargo.toml diff **仅一行** members 追加
- [ ] 内容无损测试（5.3-6）存在且通过
- [ ] `wf/bravo-import` 已推远端，commit 信息规范

## 7. 后续切片队列（第一切片验收后按序领）

1. **ENEX 解析导入**（印象笔记 XML + 内嵌资源提取；资源先落附件区——附件 API 若缺，写 request）
2. **导入器 CLI/桌面接线**（desktop 调用点——跨领地，须先 request）
3. **M3 其余格式**（Notion/HTML/OPML）+ 迁移向导 UI（防重/选择性导入/进度）

## 8. 已知坑（Alpha 踩过，直接绕行）

1. **CI -D warnings**：clippy --all-targets 是门槛，测试代码也过（run 110 教训）
2. **Write 工具落盘 root 属主问题**：python 改文件用 Edit 工具或先 chown
3. **别信对话记忆，信编译器和测试**——断言改动立即重跑
4. **S5 教训（本卡强制）**：测试必须真实执行——交付前 `cargo test -p aurora-import 2>&1 | grep "test dk09"` 逐一确认名单出现，防 cfg/feature 门控静默吞测试（DK-08 S5 曾整模块门控漏跑，验收时才抓到）
5. **磁盘 >90%**：`rm -rf target/debug/{deps,incremental}`，别 cargo clean

## 9. 工作流规约

- 分支 `wf/bravo-import`，commit 前缀 `feat(DK-09):` / `test(DK-09):`
- request 文档命名 `bravo-request-*.md` 放 issues/wf/
- 完成后推分支即止——**不自行 merge main**（Alpha 集成 + 验收打勾）
- 与 charlie（DK-10 AI 网关）、delta（DK-11 画布）零交集；领地冲突风险仅根 Cargo.toml 一行——若 push 被 CI 拒（他人并发改了 members），rebase 后重推
