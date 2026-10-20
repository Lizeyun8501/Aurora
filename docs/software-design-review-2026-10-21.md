# Aurora Note 软件设计深度审阅 v2（2026-10-21）

> 审阅基线：commit `2577120`（2026-10-16 审阅报告全部条目收官后）
> 本次范围：Bug 调研修复 / 架构先进性 / 功能完备度对标 / 主流竞品 UI/UX 提取
> 演进数据：五天 108 commits，Rust 104.6K 行（+34K），14 crate，编辑器测试 26 文件

---

## 0. Bug 调研与修复（已落地 commit `5b8d365`）

### R2-Bug1：任务投影同步三重缺陷（P1 · 已修复）

`apply_to_projection` 以**行号**派生 task_id，存在结构性三重缺陷：

| 缺陷 | 触发场景 | 用户可见后果 |
|---|---|---|
| 删行残留 | 笔记中删除任务行 | 投影旧行永存，TodoView 幽灵任务 |
| 行号漂移 | 任务行前插入任意文本 | 同一任务以新旧两个 id 双生，内容分叉 |
| 勾选重置 | TodoView 勾完成（UI 不回写文本） | 下次保存重播种 status=inbox，**静默撤销用户操作** |

**修复**：note 级重建同步——快照旧 ai:* 行全状态映射（title→status）→ `remove_rows_for_note` 清场 → 重播种时映射优先（勾完成保 done、取消勾不被文本重播回 done；UI 交互态优先于文本标记）。回归测试 2 例（重建清场断言+双向勾选保留断言），10/10 绿。

### R2-Bug2：@ 提及嵌套损坏（P2 · 已修复）

未闭合 `[[` 存续期间输入 `@` 触发补全，插入 `[[title]]` 产生嵌套损坏。修复：`openClosed` 条件——未闭合 `[[` 存续期 @ 让位。

### 体检面结论

- `cargo clippy -D warnings`：core 0 错；desktop 0 错（fakepc 环境）
- `cargo test`：core action_extractor 10/10 绿（全量 workspace 因磁盘 100% 受阻，分层验证）
- TypeScript 0 / vite build 0
- vitest 11 unhandled errors 全部为 **ENOSPC**（环境问题非代码问题）
- **环境风险**：磁盘第四次爆满（target/deps 16G——wasmtime+observability 链接面）。建议：CI 加 `cargo clean -p` 分层缓存策略 + 本机 `CARGO_INCREMENTAL=0` 跑全量验证

**未发现**：并发死锁、加密路径错误、迁移数据丢失类 P0 问题（加密面抽查水印实现——会话 ID+日粒度+本机留档，设计正确）。

---

## 1. 架构评估：是否先进？

### 1.1 先进之处（保持并强化）

| 维度 | 现状 | 评价 |
|---|---|---|
| **事件溯源+投影** | event_bus + 8 个 l2_engines 投影（任务/双链/标签/搜索…） | 读模型与权威源分离，Todoist 级架构；**先进** |
| **CRDT 底座** | Loro（多端合并/快照/checkout） | 对标 Figma/Linear 一线选型；**先进** |
| **端到端加密** | LocalDekVault+PBKDF2/Ed25519+密文内容通道 | 个人笔记软件罕见的安全档位；**先进** |
| **WASM 插件沙箱** | wasmtime+guest 签名+audit log | 对标 Obsidian 插件生态的安全做法；**先进** |
| **分层 crate** | entity/migration/core(l1/l2/l3)/security/sync/plugin/ai/observability | 边界清晰；**良好** |
| **AI 现场** | agent_context（当前文档+选区+反链+GTD）一等公民 | 对标 Notion AI 的正确姿势（现场感知>外挂问答）；**先进** |

### 1.2 优化之处（按投入产出排序）

1. **mobile-ffi 巨石化（2723 行）**——Desktop 与 Mobile 双端逻辑重复（投影播种/分享/导入各写一遍，本次 Bug1 即双端同源缺陷的实证）。建议：抽 `aurora-service` 层（core 之上、ffi 之下），ffi 只留绑定胶水。**中改动、高复利**。
2. **observability 超重（8K+ 行，占 core 外最大）**——monitoring/testing/release/diagnostics 四模块对个人应用偏企业级。建议：裁剪至 metrics+tracing 两面，其余 feature-gate。**减面**。
3. **write_path 2260 行**——写路径大杂烩（保存/重命名/派生/快照）。拆 `write/note.rs`、`write/blocks.rs`、`write/snapshot.rs`。**纯重构，低风险窗口期做**。
4. **测试金字塔偏科**——core 454+ 用例 vs 编辑器 26 文件 vs E2E 0。块编辑器是用户价值核心，往返一致性（mdToNodes/docToMd）建议补 property-based 测试（proptest 已在依赖树）。**质量杠杆最大**。
5. **磁盘面失控**——deps 16G 拖垮验证链。CI 分层缓存+`.cargo/config.toml` 关 debuginfo 溢出（`debug=1`→`debug=0, line-tables-only`）。**环境债**。
6. **双端行为一致性无守门**——同一缺陷双端重复（Bug1 实证）。建议：core 集成测试覆盖「保存→投影」全链，双端只准调 core 不准绕过。

### 1.3 结论

架构**总体先进**（事件溯源+CRDT+E2E+沙箱四件套超过绝大多数个人笔记软件），主要债在**双端复用层缺失**与**obs 超重**，无结构性翻新需求。

---

## 2. 功能完备度：对标主流优质软件

### 2.1 已覆盖关键功能矩阵（五天收官后）

| 功能域 | 对标 | Aurora 现状 |
|---|---|---|
| 块编辑器 WYSIWYG | Notion | ✅ 七切片（拖拽/合并/选区/代码/公式/mermaid/表格） |
| 双向链接+反链 | Obsidian/Roam | ✅ +知识图谱 |
| 任务 GTD+自然语言日期 | Todoist | ✅ 投影聚合+「明天 9 点」解析+@提及 |
| 导入导出 | 全员 | ✅ Markdown/ENEX |
| 分享+直传+水印 | 自研 | ✅ 配对直传+补发+捕获 |
| 浏览器剪藏 | Notion/Obsidian Web Clipper | ✅ 全链 |
| 全局快速捕获 | Bear/Todoist Quick Add | ✅ Alt+Shift+N |
| 命令面板 | Obsidian/Notion | ✅ Mod+K |
| 插件系统 | Obsidian | ✅ WASM 沙箱（guest 签名+审计） |
| E2E 加密 | 标准笔记 | ✅（多数竞品无） |
| AI 现场 | Notion AI | ✅ MCP+agent context |

### 2.2 待补充（按价值排序）

| # | 缺口 | 对标 | 级别 | 说明 |
|---|---|---|---|---|
| G1 | **数据库/表格视图** | Notion database | P1 | 现有表格=静态表。块属性+多视图（表/看板/日历）是 Notion 护城河；Aurora 有 blocks+投影底座，做「属性化块+视图投影」路线成本可控 |
| G2 | **每日笔记（journal 流）** | Roam/Logseq/Obsidian Daily | P1 | 零模板成本：`daily/2026-10-21` 自动建页+侧栏入口+今日任务自动汇聚。是 clipper/快速捕获的天然落地页（当前捕获落 `#capture` 标签页，改为落当日 journal 更符合直觉） |
| G3 | **闪卡/间隔重复 UI** | 思源 SR/Anki | P1 | **fsrs.rs 418 行算法在库但零 UI 接入**——最便宜的 P1（算法已自研）。块→卡+复习面板+队列三件 |
| G4 | **任务看板/日历视图** | Todoist/Notion | P1 | TaskProjection 数据面已齐（by_status/today/due_date），缺的只是视图渲染。Kanban（inbox/next/waiting/done 四列拖拽）+周历两视图 |
| G5 | **块引用（transclusion）** | Roam/Logseq/思源 | P2 | 现有 `![[嵌入]]` 页级；块级引用+聚焦是双链深水区 |
| G6 | **公开发布（publish）** | Obsidian Publish/Notion Site | P2 | 公开只读链接已在挂起池；配合水印链路可直接兑现 |
| G7 | **高级搜索语法** | Obsidian search | P2 | nl_query 已有本地规则；补 `path:/tag:/status:` 运算符即可 |
| G8 | **协作/多人** | Notion 实时协同 | P3 远期 | Loro CRDT 底座已在，缺同步服务端+权限模型 |
| G9 | **模板系统增强** | Notion/Templater | P2 | 现有模板插入；缺变量（`{{date}}`）与模板触发器 |
| G10 | **图片/PDF 深度管理** | Obsidian | P2 | asset_library 已有；缺图床外链、PDF 标注回链 |

**总评**：核心笔记+任务+双链+分享链已**覆盖优质软件关键面的 80%**；最大空白=数据库视图（G1）与每日笔记（G2）——前者大后者小，建议 G2/G3/G4（三个 P1 中最便宜的两个先做：G3 算法已在、G4 数据已在）。

---

## 3. 主流竞品 UI/UX 提取（可落地的改进点）

### 3.1 逐家提取

**Todoist**（任务面——Aurora 最直接对手）
- ✦ **自然语言输入实时高亮**：输入「明天 9 点」时该片段即时高亮+chip 预览解析结果——Aurora 3.4 已有解析引擎，**只差输入时预览 UI**（低成本高感知，强烈建议）
- ✦ 今日视图三分区：逾期/今天/后续——Aurora today() 已返回逾期含糊集，UI 分区即可
- ✦ Karma 连击：动机设计（P2，个人产品粘性）

**Obsidian**（知识面）
- ✦ 命令面板模糊评分（fuzzy score）+ 最近使用置顶——Aurora palette 现为子串过滤
- ✦ 标签面板+层级标签树（#a/b/c）——tags_projection 已有数据
- ✦ 大纲面板（当前文档标题树+点击跳转）
- ✦ 工作区布局保存/切换（多窗口场景）

**Notion**（块面）
- ✦ 块 hover 手柄（⠿ 拖拽+菜单合一）——2.7 已有拖拽，手柄视觉可再对标
- ✦ 页面面包屑（层级路径点击跳转）——树结构已有，UI 缺
- ✦ Cover+图标（页面门面，低技术成本高感知）

**Logseq/Roam**（大纲流）
- ✦ 块级缩进大纲模式（outline-first 输入）——与现有块模型兼容的 alternate view，P2
- ✦ 块聚焦（zoom-in）：点击任意块全屏只看该块子树——深度写作场景利器

**思源**（本土化）
- ✦ 闪卡复习面板（G3 的 UI 形态参照）
- ✦ 集市（插件市场）——插件系统成熟后的生态面

### 3.2 UI 改进优先级（四象限）

| 优先 | 项 | 成本 | 感知 |
|---|---|---|---|
| ⭐⭐⭐ | NL 日期输入实时预览（Todoist） | S | 极高（3.4 的临门一脚） |
| ⭐⭐⭐ | 今日视图逾期/今天/后续三分区 | S | 高 |
| ⭐⭐ | 页面面包屑 | S | 高（树数据已有） |
| ⭐⭐ | 闪卡复习面板（G3） | M | 高（算法已在） |
| ⭐⭐ | 看板视图（G4） | M | 高（数据已在） |
| ⭐ | 模糊评分 palette | S | 中 |
| ⭐ | Cover+图标 | S | 中 |
| ⭐ | 块聚焦 zoom-in | M | 中 |
| ⭐ | 暗亮主题（3.8 v2） | M（token 层重构） | 中 |

### 3.3 结论

Aurora UI 底子扎实（块编辑器+树侧栏+命令面板齐备），**感知差距集中在「数据已有但视图/预览缺位」**——NL 日期预览、今日三分区、看板、闪卡面板四个点全部是「数据面已就绪只差 UI」的高杠杆项。

---

## 4. 路线建议（若继续投入）

**第一批（S 级速赢，1-2 轮）**：NL 日期实时预览 → 今日视图三分区 → 页面面包屑
**第二批（P1 功能，各一卡）**：G2 每日笔记 → G3 闪卡面板 → G4 看板
**第三批（架构还债）**：aurora-service 双端统一层 → write_path 拆分 → proproperty-based 编辑器测试
**远期**：G1 数据库视图（最大单卡）→ G6 发布 → G8 协作
