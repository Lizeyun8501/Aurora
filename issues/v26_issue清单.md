# Aurora Note V26.0 — GitHub Issue 清单

> 本文件是 Issue 的**唯一真相源**。`create_issues.py` 直接解析本文件调用 `gh issue create`。
> 修改 Issue 只需改本文件，重新跑脚本即可（脚本支持幂等更新，见文末说明）。

**基线**：远端 `main@4c70dbeb`（2026-09-09）· V26.0 架构与产品设计规格
**总数**：29 个 Issue（5 修订卡 + 22 域卡 + 2 本版补卡）
**总人日**：约 840（并行后关键路径约 650）

---

## 0. 初始化：标签与里程碑

提交 Issue 前先执行一次。

```bash
REPO=Lizeyun8501/Aurora

# 优先级
gh label create "P0" -R $REPO -c "B22222" -d "阻断级：不解决则后续工作失去基础" --force
gh label create "P1" -R $REPO -c "FBCA04" -d "高：功能完整性与安全生态" --force
gh label create "P2" -R $REPO -c "0E8A16" -d "中：体验与优化" --force

# 类型
for t in "type/bug:缺陷" "type/feat:新功能" "type/refactor:重构" "type/chore:工程" "type/docs:文档"; do
  gh label create "${t%%:*}" -R $REPO -c "C5DEF5" -d "${t##*:}" --force
done

# 领域
for a in "area/ci:CI与门禁" "area/arch:架构与契约" "area/core:微内核" "area/storage:存储与迁移" \
         "area/editor:编辑器" "area/ui:界面与交互" "area/mobile:移动端" "area/desktop:桌面端" \
         "area/sync:同步" "area/security:安全与加密" "area/ai:AI与Agent" "area/plugin:插件生态" \
         "area/i18n-a11y:无障碍与国际化"; do
  gh label create "${a%%:*}" -R $REPO -c "D4C5F9" -d "${a##*:}" --force
done

# 特殊标记
gh label create "静默占位" -R $REPO -c "5319E7" -d "能编译、能跑、看起来正常，但实际未实现" --force

# 里程碑
gh api -X POST repos/$REPO/milestones -f title="M0 · Phase 0 止血与收敛" -f description="CI止血 + 契约冻结 + 写入路径统一 + 前端收敛" >/dev/null
gh api -X POST repos/$REPO/milestones -f title="M1 · Phase 1 核心基石" -f description="存储迁移 + 编辑器 + 组织结构 + 检索 + 去占位" >/dev/null
gh api -X POST repos/$REPO/milestones -f title="M2 · Phase 2 任务与安全" -f description="GTD + 三级加密 + 同步路由 + 知识网络" >/dev/null
gh api -X POST repos/$REPO/milestones -f title="M3 · Phase 3 效能与智能" -f description="AI液化 + Agent + 导入导出 + 画布" >/dev/null
gh api -X POST repos/$REPO/milestones -f title="M4 · Phase 4 集成与开放" -f description="插件系统 + 协作分享 + 备份恢复" >/dev/null
gh api -X POST repos/$REPO/milestones -f title="M5 · Phase 5 生态优化" -f description="移动端体验 + 无障碍达标 + 主题与快捷键" >/dev/null
```

---

## 1. 依赖关系图

```
R-00 (CI止血) ─┬─> R-01 (文档) ──┐
               ├─> R-02 (契约) ──┴─> DK-00 (契约冻结)
               ├─> R-04 (排期)                 │
               └─> DK-05M-V (WebView验证)      │
                                               ├─> DK-01W (写入路径统一) ─┐
                                    DK-0F (前端收敛) ───────────────────┤
                                                                        │
                                                            DK-01 (存储迁移) ─┬─> DK-02 组织结构
                                                                               ├─> DK-03 检索 ─> DK-04 知识网络
                                                                               ├─> DK-06 GTD
                                                                               ├─> DK-07 安全
                                                                               ├─> DK-08 同步 ─> DK-13 协作
                                                                               ├─> DK-09 导入导出
                                                                               ├─> DK-11 画布
                                                                               ├─> DK-12 结构化
                                                                               └─> DK-17 备份恢复
                                    DK-00 ─> DK-05 (桌面编辑器) / DK-05M (移动编辑器, 依赖 DK-05M-V)
                                           ─> DK-10 AI / DK-14 插件 / DK-16 去占位 / DK-18 设置
```

**唯一串行瓶颈**：`R-00`。它之后 A/B/C 三线可并行：
- **A 线**：R-01 + R-04（纯文档）
- **B 线**：R-02 → DK-00（契约）
- **C 线**：DK-05M-V（WebView 验证，越早越好）

---

## 2. 修订卡（R 系列）

## [R-00] CI 止血：五个 job 全红，门禁已失守

**Labels**: `P0`, `area/ci`, `type/bug`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 1.5–4 人日
**Blocked by**: —
**Assignee**: —

### 背景

实测远端 `main@4c70dbeb` 的 GitHub Actions 五个 job **全部 failure**，且不是偶发——每个 job 的执行步骤都失败。

| Job | 失败步骤 | 根因 |
|---|---|---|
| MSRV (1.85) | Check (MSRV) | `rust-toolchain.toml` 锁 1.85；但 loro 需 **1.87+**、iroh 需 **1.91+**，而 `mobile-ffi` **非可选**地启用 `loro-crdt` → 1.85 必然编译失败 |
| Test (stable) | Build | `crates/migration/src/lib.rs:579` 写 `#[path = "tests_v2.rs"]`，但文件实际在 `src/tests/tests_v2.rs` |
| Clippy | all-targets | 同上（测试编译失败）+ 待清理的告警 |
| Rustfmt | `cargo fmt --check` | 格式漂移 |
| desktop-check | `cargo check -p aurora-desktop` | 独立编译失败（需本地复现确认是 Tauri 系统库还是代码问题） |

**五个 job 全红意味着没有任何一次提交被验证过可编译。** 在这种状态下，新代码是否破坏既有能力无法判断，"已实现"的标注失去验证基础，任何排期估算都建立在未验证的地基上。

### 任务

- [ ] 工具链统一到 **1.91**（与 `Cargo.toml` 注释声明一致），同步改 CI MSRV job
- [ ] 修 `migration/src/lib.rs:579` 为 `#[path = "tests/tests_v2.rs"]`
- [ ] `cargo fmt --all`，提交格式修正
- [ ] 本地复现 `desktop-check` 失败，定位是系统库还是代码问题
- [ ] 清 clippy 告警至 `cargo clippy -D warnings` 通过
- [ ] 开启 branch protection：任一 job 变红禁止合并

### DoD / 验收标准

- [ ] 五个 job 全部 green
- [ ] `cargo check --workspace` 与 `cargo test --workspace` 在本地与 CI 均通过
- [ ] branch protection 已开启，`main` 禁止 force push

> **本卡优先于所有其他工作。** 总人日 1.5–4 天，是全项目性价比最高的一项。

---

## [R-01] 文档修复：编号同源、删除虚数、补回设计原则

**Labels**: `P1`, `type/docs`, `area/arch`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 1–2 人日
**Blocked by**: R-00
**Assignee**: —

### 背景

V25 存在两处影响可用性的文档缺陷：

1. **编号体系崩溃** —— 目录用 `2.6–2.14 / 5.22–5.29` 全局连续编号，正文用 `1.1–1.5 / 2.1–2.4`（重复两轮）/ `5.1`（出现四次）。研发说"参见 5.26"，翻到正文看到第四个 `5.1`，无法定位。
2. **四处计数错误** —— 端口标称 21 实为 20、实体标称 28 实为 32、表标称 34 实为 37、域十七标称 18 实为 15。**四个全错且方向不一致**。

顺带记录一个值得警惕的现象：唯一算对的两个数是功能点 412 和人日 1160——因为它们是**从明细累加**的；而 21/28/34/18 是**先写整数再往里填内容**。

### 任务

- [ ] 统一编号：目录与正文同源，每部分内重置
- [ ] 删除凭感觉的整数：能列全的逐一列出，必须给总数时由明细累加
- [ ] 补回五条设计原则（V22.1 有、V25 在合并中丢失）
- [ ] 视觉设计系统前移至界面部分开头
- [ ] 新增域—屏映射索引、术语表、变更记录

### DoD

- [ ] 全文无重复编号，`grep` 校验每个"X.Y"唯一
- [ ] 封面与正文的所有数字均可追溯至明细表
- [ ] 设计原则五条可验证判据齐备

---

## [R-02] 契约补全：事件字典 / 端口签名 / CoreAPI / 62 错误码

**Labels**: `P0`, `area/arch`, `type/feat`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 8–12 人日
**Blocked by**: R-00
**Assignee**: —

### 背景

V26 定义了"有哪些端口"，但**没定义端口长什么样**。20 个端口只有名字和一行说明，无 Trait 方法签名、无 CoreAPI、无事件字典。

文档自述"契约冻结后三条流水线并行开工"——DK-00 要冻结的东西，此前根本没有内容。

更严重的是**退步**：V22.1 有 46 个错误码及"用户文案/是否重试/降级动作"三元组，V25 里一个都没有。错误码是本地优先产品的生命线。

### 任务

- [ ] 事件字典 **41 类**：沿用远端 `CoreEvent` 的 PascalCase 与 Rust↔TS 镜像实践（`event.rs` ↔ `events.ts`），**扩展而非另起炉灶**
- [ ] 端口 Trait 签名：核心方法**不提供默认实现**，编译期强制适配器给出真实实现
- [ ] CoreAPI TypeScript 契约：以 mobile-ffi 现有 14 个 JNI 函数为第一版基线
- [ ] 错误码 **62 个**（A12+B12+C10+D10+E8+F6+G4），每个给「用户文案 / 是否重试 / 降级动作」三元组

### DoD

- [ ] 41 类事件全部落库，Rust 与 TS 严格镜像（现有 12 变体命名不变）
- [ ] `SyncTarget` 等核心端口的关键方法无默认体，`cargo check` 强制适配器实现
- [ ] 62 个错误码进入统一 `Error` 枚举，`user_message()` / `is_retryable()` / `requires_fallback()` 三问可测
- [ ] 契约变更需走 ADR，禁止口头约定

---

## [R-03] 缺口补齐：七项关键功能 / 权威源 / 并发模型

**Labels**: `P1`, `area/arch`, `type/feat`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 3–5 人日
**Blocked by**: R-02
**Assignee**: —

### 背景

V25.1 审研发现在"优质软件关键功能"上缺七项。同时有三处架构层讨论缺失。

### 任务

七项关键功能（详见各域卡）：

- [ ] **每日笔记自动创建** —— 日记是最高频写入场景，手动建模板等于没有
- [ ] **单篇笔记加密 Vault** —— "只锁这一篇"需求常见，为它单开 Private 工作区过于笨重
- [ ] **任务依赖（前置任务）** —— V22 曾提及，V25 重构中丢失，是"版本合并丢内容"的实例
- [ ] **时间追踪**（预计 vs 实际）—— 周回顾的核心输入
- [ ] **附件内置预览器** —— 无预览器的附件列表等于文件管理器
- [ ] **Markdown 源码模式** —— 让"开放格式"承诺变得可见
- [ ] **用户自定义主题** —— 仅允许覆盖设计令牌白名单变量，禁任意 CSS 注入

三处架构补齐：

- [ ] `notes.content` 与 `blocks` **权威源三阶段定义** + 崩溃恢复规则
- [ ] 并发模型与多窗口（V25 通篇未讨论）
- [ ] 产品态 Schema 演进（现有 M1–M3 只是从当前代码迁移，不是用户 v1→v2 升级）

### DoD

- [ ] 七项功能全部进入功能域并有对应域卡
- [ ] 权威源每阶段只有一个，另一方为派生；派生失败不阻断主流程
- [ ] 并发规则写入铁律：**任何合并不允许丢弃用户已确认的输入**

---

## [R-04] 排期重排：移动端编辑器独立卡 / 无障碍拆入 DoD

**Labels**: `P1`, `type/chore`, `area/arch`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 2 人日
**Blocked by**: R-00
**Assignee**: —

### 背景

V25 排期有三处结构性问题：

1. **移动端编辑器无独立卡** —— "WebView 输入时序"被列为 Phase 0 必做验证项，但这个全项目技术密度最高的模块被塞进 DK-15 的 90 人日里，和导航、手势、小组件共享预算。**这不是排期，是祈祷。**
2. **无障碍排期会做不成** —— DK-16 依赖"全部 UI"，排在所有界面之后。后补成本是内建的 3–5 倍，且无法靠补几个 `aria-label` 通过 VoiceOver 遍历。
3. **Phase 0 验证项无人认领** —— "WebView 输入/滚动验证"与"Mirror 三压力场景"两项必做验证，没有任何卡认领。

### 任务

- [ ] 新增 **DK-05M** 移动端块编辑器独立卡（70 人日）
- [ ] 新增 **DK-05M-V** WebView 输入时序验证（Phase 0 首个子任务，5 人日）
- [ ] 取消无障碍独立卡，**A1–A9 拆进每个 UI 相关卡的 DoD**
- [ ] 国际化 I1–I6 进 DK-00 契约层（RTL 影响布局、复数影响文案结构，必须契约期决定）
- [ ] WebView 验证 → DK-05M-V；Mirror 三压力场景 → DK-09

### DoD

- [ ] 每个 UI 卡的 DoD 含对应无障碍验收项
- [ ] 两项 Phase 0 验证项均有明确归属卡
- [ ] 排期表无"依赖全部 UI"这类无法完成的依赖声明

---

## 3. 补卡（V26 表中缺失但 Phase 0 必需）

> 以下两张卡是我在整理 Issue 时发现的遗漏：V26 表 7-4 把"写入路径统一"与"前端收敛"列为 Phase 0 核心交付，但表 7-3 的 DK 系列里**没有对应卡**。无卡则无人认领、无法验收，因此补上。

## [DK-01W] 写入路径统一：桌面端与移动端当前是两条互不相通的路

**Labels**: `P0`, `area/arch`, `area/core`, `type/refactor`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 12–18 人日
**Blocked by**: DK-00
**Assignee**: —

### 背景

这是 V26 最重要的架构修订。实测两条写入路径：

| 项 | 移动端（mobile-ffi） | 桌面端（`cmd_create_note`） |
|---|---|---|
| 写模型 | `NoteDoc`（Loro CRDT） | **裸 JSON** |
| 存储 key | `notesnap:{id}` | `note:{id}` |
| 序列化 | Loro snapshot | `serde_json::json!` |
| blocks 双写 | 有（`blocks.sync_note_blocks`） | **无** |
| EventBus 事件 | 有 | **无** |
| 投影更新 | 事件驱动 | 手动调 `index_note_in_search` |

**后果是连锁的**：
- 铁律"写模型唯一"被违反
- 搜索/双链/任务投影收不到桌面端变更（**TodayView 在桌面端永远是空的**）
- **两端数据无法互相同步**——格式与 key 都不同
- 同步层做得再完整，也只能同步移动端那一半

### 任务

- [ ] 实现 `WritePath` 唯一写入入口（`load → apply → 原子保存 → 派生 blocks → 发事件`）
- [ ] 桌面端 `cmd_*` 全部改走 `WritePath`，**禁止直接调用 `kv_store.set`**
- [ ] 统一 key 命名与序列化格式（桌面与移动一致）
- [ ] 投影一律由事件驱动，删除 command 里手动触发索引的代码
- [ ] 三步原子事务：tmp → fsync → rename，`pending_writes` 崩溃重做

### DoD

- [ ] 桌面端与移动端调用**同一个** `apply_note_change`
- [ ] 桌面端创建的笔记能被搜索、双链、任务投影读到（**TodayView 在桌面端有数据**）
- [ ] 两端数据格式与 key 一致，互相可同步
- [ ] 故障注入测试：断电/磁盘满/杀进程后三者状态一致，零丢失

> **不先做这张卡就进 Phase 1，等于在两套互不相通的数据模型上同时盖楼。** 等到 Phase 2 做同步时才发现两端无法互通，返工成本是现在的数倍。

---

## [DK-0F] 前端收敛：三前端两编辑器栈，共享层零引用

**Labels**: `P0`, `area/ui`, `area/desktop`, `type/refactor`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 10–15 人日
**Blocked by**: DK-00
**Assignee**: —

### 背景

实测前端格局：

| 项目 | 编辑器 | 实际用途 |
|---|---|---|
| `apps/web` | TipTap（**无 CRDT**） | **Tauri 实际加载**（`frontendDist: "../web/dist"`） |
| `apps/desktop/src` | 无编辑器 | 有 React Shell，但 **Tauri 不加载它** |
| `apps/mobile` | 原生 ProseMirror + loro-prosemirror | **唯一接 Loro 的一端** |
| `shared/ui-components` | TipTap | package.json 声明 + vite alias 配置，**但全仓零 import** |

**桌面端名为 desktop，实则运行 web 产物。** 由此推出：**Loro CRDT 只接在移动端编辑器上，桌面端无任何 CRDT 绑定。**

而 `docs/adr/ADR-001` 写得很好——统一 React 18 + Zustand、共享层沉淀、UI 复用率 ≥70%、退出条件"双端编译门禁绿"。**ADR 是对的，执行是空的。**

### 任务

采用**方案 A**（需在 ADR 中正式记录）：

- [ ] 保留 `apps/web` 作为**唯一前端运行时产物**
- [ ] **删除** `apps/desktop/src`（或明确降级为设计稿存档并从构建中移除）
- [ ] `apps/web` 接入 `@aurora/ui-components`，使共享层被真实引用
- [ ] 编辑器统一为 **ProseMirror + loro-prosemirror**（从 mobile 上移，TipTap 降为可选依赖）
- [ ] 补充 ADR 记录本次收敛决策与退出条件

### DoD

- [ ] `frontendDist` 指向唯一产物，无歧义
- [ ] `shared/ui-components` 至少被 1 处真实 import（不为零）
- [ ] 桌面端编辑器与 Loro CRDT 有绑定关系
- [ ] 双端编译门禁绿

### 已知代价

`shared/ui-components` 当前依赖 TipTap（`@tiptap/react ^2.10`），而 mobile 用原生 ProseMirror。统一到 ProseMirror 后，**共享层的编辑器部分需重写**，但块渲染器、布局原语、stores、hooks 可直接复用。

> 本方案是 **ADR-001 的真正落地，而非新的方向选择**。

---

## 4. 域卡（DK 系列）

## [DK-00] 契约层冻结

**Labels**: `P0`, `area/arch`, `area/core`, `type/chore`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 8 人日
**Blocked by**: R-02
**Assignee**: —

### 背景

契约是三条流水线并行开发的前提。契约不定，11 个能力组件无法并行——它们只能等。

### 任务

- [ ] 事件字典 41 类落库，Rust↔TS 严格镜像
- [ ] 20 个端口 Trait 签名落库，核心方法无默认实现
- [ ] CoreAPI TypeScript 契约落库
- [ ] 62 个错误码进入统一 `Error` 枚举
- [ ] 国际化 I1–I6 在契约期决定（RTL / 复数 / 日期格式）

### DoD

- [ ] 全部契约可编译，双端注入一致
- [ ] 契约变更必须走 ADR，禁止口头约定
- [ ] 契约冻结后，B/C 线可并行开工

---

## [DK-01] 存储与迁移

**Labels**: `P1`, `area/storage`, `area/core`
**Milestone**: `M1 · Phase 1 核心基石`
**Estimate**: 30 人日
**Blocked by**: DK-00, DK-01W
**Assignee**: —

### 任务

- [ ] M1–M3 三段式迁移（新增 → 双写 → 切读 → 观察一版 → 删旧，**禁止直接改既有表后删列**）
- [ ] 三步原子事务（tmp → fsync → rename）+ `pending_writes` 崩溃重做 + SHA 校验修复
- [ ] 权威源三阶段规则落地（M3 期 `notes.content` 为权威，blocks 为派生；终态 Loro 为权威）
- [ ] 投影水位线表，启动时对比事件流决定是否重建
- [ ] 目录树表补齐（`notebooks` / `tags` / `bookmarks` 曾在 V24 迁移脚本中整体缺失）

### DoD

- [ ] 故障注入（断电/磁盘满/杀进程）后数据零丢失
- [ ] 每个阶段只有一个权威源，派生失败不阻断主流程
- [ ] 迁移可回滚（预迁移快照）

---

## [DK-02] 组织与结构（域一）

**Labels**: `P1`, `area/ui`, `area/core`
**Milestone**: `M1 · Phase 1 核心基石`
**Estimate**: 35 人日
**Blocked by**: DK-01
**Assignee**: —

### 任务

- [ ] 无限层级目录树（`notes.parent_id` 自引用 + `kind` 区分笔记本/笔记）
- [ ] **环检测**：把父节点拖进自己子树会让整棵树消失，这是目录树最经典的致命 bug
- [ ] 标签管理（层级标签、使用计数、批量整理）
- [ ] 智能文件夹（保存的查询条件）
- [ ] 书签（笔记 / 笔记本 / **保存的搜索**）
- [ ] **回收站**：trash 存 `origin_path` 快照 + 主表打标记，还原到原位置

### DoD

- [ ] 环形移动被拒绝并给出可读提示
- [ ] 回收站显示原位置路径，还原回原位而非根目录
- [ ] 树查询用递归 CTE + `(workspace_id, parent_id, sort_order)` 复合索引
- [ ] 无障碍：树节点可键盘展开/聚焦，VoiceOver 可遍历

> **回收站优先级最高。** 16 款竞品全具备，本地优先产品误删即永久损失。

---

## [DK-03] 检索（域三）

**Labels**: `P1`, `area/core`, `area/ui`
**Milestone**: `M1 · Phase 1 核心基石`
**Estimate**: 25 人日
**Blocked by**: DK-01
**Assignee**: —

### 任务

- [ ] FTS5 主路径 + 中文分词
- [ ] 向量增强（`sqlite-vec` 优先；lancedb 降为 feature-gate 可选，见 DK-16）
- [ ] 混合检索 BM25 + 向量，RRF 融合
- [ ] 可选 BGE-reranker 精排
- [ ] **语义召回徽章**：不含关键词的结果必须解释来源，否则用户不信任搜索

### DoD

- [ ] 万条笔记全文搜索 <200ms（桌面）/ <400ms（移动）
- [ ] 混合检索 <500ms（桌面）
- [ ] Private 工作区解锁态内存建索引，关闭即销毁

---

## [DK-04] 知识网络（域五）

**Labels**: `P2`, `area/core`, `area/ui`
**Milestone**: `M2 · Phase 2 任务与安全`
**Estimate**: 20 人日
**Blocked by**: DK-03
**Assignee**: —

### 任务

- [ ] 双链解析与反向链接维护
- [ ] **N 跳图遍历**（当前仅正/反向索引，缺图遍历）
- [ ] 知识图谱可视化（力导向 + 聚焦高亮 + 分层 LOD）

### DoD

- [ ] 反链与搜索索引一致（Medium 通道先于 Low，否则搜到笔记却看到失效反链）
- [ ] 万级节点图谱可交互

---

## [DK-05] 块编辑器（桌面端）

**Labels**: `P0`, `area/editor`, `area/desktop`, `area/i18n-a11y`
**Milestone**: `M1 · Phase 1 核心基石`
**Estimate**: 90 人日
**Blocked by**: DK-00, DK-0F
**Assignee**: —

### 任务

- [ ] 16 种块类型（文本/标题/任务/图片/代码/引用/画布/文件/嵌入等）
- [ ] **Markdown 源码模式**（CodeMirror 6，与块视图双向切换，保留光标位置）
- [ ] 悬浮工具栏 + 选中 AI 菜单
- [ ] 全本地渲染禁 CDN（代码高亮 / KaTeX / Mermaid）
- [ ] 无障碍：键盘可达、焦点可见、语义标签由 VM 携带

### DoD

- [ ] 源码修改后切回块视图正确重建；非法 Markdown 不崩溃
- [ ] 断网状态下所有渲染可用
- [ ] 键盘可完成全部编辑操作

---

## [DK-05M-V] WebView 输入时序验证（Phase 0 必做）

**Labels**: `P0`, `area/mobile`, `area/editor`, `type/chore`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 5 人日
**Blocked by**: R-00
**Assignee**: —

### 背景

V25 把 "WebView 输入/滚动验证" 列为 Phase 0 必做项，但**没有任何任务卡认领**。这是全项目技术密度最高的模块（ProseMirror + Loro + WebView 输入法），不验证就开工风险极高。

### 任务

- [ ] 验证输入法候选、键盘弹起高度、输入事件时序与原生差异
- [ ] 验证长文档滚动性能与惯性滚动差异
- [ ] 验证 WebView 存储被系统清理的边界（**关键数据必须存原生 SQLite，禁依赖 localStorage**）
- [ ] 确认编辑器技术选型是否需要调整

### DoD

- [ ] 输出验证报告，明确"可行 / 需调整 / 不可行"
- [ ] 若需调整，同步更新 DK-05M 的技术方案
- [ ] 输入时序问题的规避方案已记录

---

## [DK-05M] 移动端块编辑器

**Labels**: `P0`, `area/mobile`, `area/editor`, `area/i18n-a11y`
**Milestone**: `M1 · Phase 1 核心基石`
**Estimate**: 65 人日
**Blocked by**: DK-00, DK-05M-V
**Assignee**: —

### 任务

- [ ] ProseMirror + loro-prosemirror 在移动端落地
- [ ] **键盘上方工具条**（移动端富文本唯一可用方案——顶部工具栏在键盘弹起时够不着）
- [ ] 选中浮动菜单不被键盘遮挡
- [ ] 安全区适配
- [ ] 全本地渲染

### DoD

- [ ] 输入法候选期间光标不跳动
- [ ] 键盘工具条覆盖全部常用格式操作
- [ ] 万字笔记滚动 ≥50fps
- [ ] VoiceOver / TalkBack 可遍历

---

## [DK-06] 任务与 GTD（域四）

**Labels**: `P1`, `area/core`, `area/ui`
**Milestone**: `M2 · Phase 2 任务与安全`
**Estimate**: 60 人日
**Blocked by**: DK-01
**Assignee**: —

### 任务

- [ ] GTD 状态机（inbox / next / waiting / scheduled / someday / done / cancelled）
- [ ] **任务依赖**（前置任务）+ **环检测**
- [ ] **时间追踪**（`estimate_minutes` / `actual_minutes`），周回顾对比预计 vs 实际
- [ ] FSRS 间隔重复
- [ ] 番茄钟与专注模式
- [ ] TodayView 知识—行动同屏
- [ ] **双锚定**：`tasks.block_id` + `note_id`，任务保留来源笔记上下文

### DoD

- [ ] 环形依赖创建被拒绝并给出可读提示
- [ ] 计时期间切后台不丢数据（WebView 后台挂起约束下的补偿）
- [ ] 子任务进度由子任务推导，**禁止手改 progress 字段**
- [ ] 周回顾展示预计 vs 实际偏差率

---

## [DK-07] 安全与加密（域十二）

**Labels**: `P1`, `area/security`, `area/core`
**Milestone**: `M2 · Phase 2 任务与安全`
**Estimate**: 45 人日
**Blocked by**: DK-01
**Assignee**: —

### 任务

- [ ] 三级工作区加密（Private / Shared / Public）
- [ ] **单篇笔记加密 Vault**（笔记级 `encryption` 字段，锁定时不进任何索引）
- [ ] 审计哈希链（已完成，需补测试）
- [ ] Argon2id 参数校验、AES-GCM IV 防重用、内存 zeroize 自动化测试
- [ ] ML-KEM 互操作测试与侧信道审计

### DoD

- [ ] Private 工作区关闭后索引完全销毁
- [ ] 云端 AI 请求对 Private 工作区被阻止
- [ ] **C03 密文校验失败一律拒绝返回明文（fail-closed）**
- [ ] 锁定态下搜索/图谱/导出均不可见加密内容

---

## [DK-08] 同步（域十一）

**Labels**: `P1`, `area/sync`, `area/core`
**Milestone**: `M2 · Phase 2 任务与安全`
**Estimate**: 50 人日
**Blocked by**: DK-01
**Assignee**: —

### 任务

- [ ] SyncRouter 四路选路（iroh P2P / LAN 直连 / WebDAV / S3）+ 健康探测
- [ ] **三个适配器覆写 `recv_update` / `sync_version`** —— 当前默认实现返回空 Vec / None，增量同步实际从未发生
- [ ] 冲突处理：CRDT 自动合并；真冲突生成 `.conflict-{ts}.md` 副本
- [ ] 移动端仅 Wi-Fi 同步（可配置）
- [ ] 大文件分片 + 断点续传

### DoD

- [ ] 增量同步真实发生（非全量回退）
- [ ] 网络切换（WiFi↔蜂窝）会话保持
- [ ] 3–10 节点多 NAT 仿真测试通过
- [ ] **任何合并不允许丢弃用户已确认的输入**

> 当前 `state()` 已修复为 `sync_version` + 3s 超时真实探测（V23-I0，方向正确），但三个适配器仍无 `recv_update` 覆写。

---

## [DK-09] 导入导出与迁移向导

**Labels**: `P1`, `area/core`, `area/ui`
**Milestone**: `M3 · Phase 3 效能与智能`
**Estimate**: 30 人日
**Blocked by**: DK-01
**Assignee**: —

### 任务

- [ ] 12 种导入源（Markdown / Notion / ENEX / HTML / OPML 等）
- [ ] 7 种导出格式（Markdown / HTML / PDF / JSON / DOCX 等）
- [ ] **Mirror 三压力场景**（Phase 0 验证项，V25 列为必做但无人认领）：
  - VSCode 全局替换
  - `git checkout` 整目录回滚
  - 双端并发编辑
- [ ] Mirror 事件级落盘（停止编辑后 3 秒防抖单篇重写，**禁止 15 分钟批量快照**）

### DoD

- [ ] 三个压力场景全部通过
- [ ] 导出 Markdown 零障碍、永不设限（**道德底线**）
- [ ] Mirror 首发单向，反向导入单独立项

> **Mirror 必须事件级落盘，不能用批量快照**：15 分钟批量会让用户看到旧数据并在其上修改，然后被覆盖。

---

## [DK-10] AI 液化与 Agent

**Labels**: `P2`, `area/ai`, `area/ui`
**Milestone**: `M3 · Phase 3 效能与智能`
**Estimate**: 55 人日
**Blocked by**: DK-00
**Assignee**: —

### 任务

- [ ] **两段式提交**：`aiLiquify` 出提案 → 用户勾选 → `aiCommit` 才落库（铁律：AI 不得静默写入）
- [ ] AI 路由器：Private 强制本地；Shared 可选云端；Public 默认云端
- [ ] MCP 网关（鉴权**分级**：stdio/回环默认信任，仅对外 HTTP 走 OAuth/HMAC）
- [ ] Agent 会话 15 分钟限时 + 审计 + Kill-Switch
- [ ] 工具调用时间轴可视化

### DoD

- [ ] 关闭全部 AI 后，软件仍是完整可用的本地笔记 + GTD 工具
- [ ] AI 不出现在任何核心写入路径上
- [ ] MCP 兼容 Claude Desktop / Cursor 等主流客户端
- [ ] Agent 越权操作被拦截并记审计

---

## [DK-11] 画布（域七）

**Labels**: `P2`, `area/ui`, `area/core`
**Milestone**: `M3 · Phase 3 效能与智能`
**Estimate**: 45 人日
**Blocked by**: DK-01
**Assignee**: —

### 任务

- [ ] 无限画布 + LOD 分层渲染 + 视口裁剪
- [ ] 节点内容引用 `content_ref`（**不内嵌正文**），布局数据单独立文档
- [ ] 三种布局（自由 / 思维导图树形 / 网格）
- [ ] 导出 SVG / PNG / JSON / Markdown
- [ ] Canvas2D 起步，WebGL 置 Phase 5

### DoD

- [ ] 1000 节点渲染 ≥30fps（桌面）/ ≥25fps（移动）
- [ ] 10000 节点加载 <2s
- [ ] 画布数据可 CRDT 协同编辑

---

## [DK-12] 结构化数据（域六）

**Labels**: `P2`, `area/ui`, `area/core`
**Milestone**: `M3 · Phase 3 效能与智能`
**Estimate**: 30 人日
**Blocked by**: DK-01
**Assignee**: —

### 任务

- [ ] 属性视图四视图（表格 / 看板 / 日历 / 画廊）
- [ ] 筛选与排序
- [ ] `databases` / `db_rows` / `db_views` 三表

### DoD

- [ ] 与笔记、任务共享同一实体模型
- [ ] 视图切换不丢筛选条件

---

## [DK-13] 协作与分享（域十三）

**Labels**: `P2`, `area/ui`, `area/sync`
**Milestone**: `M4 · Phase 4 集成与开放`
**Estimate**: 30 人日
**Blocked by**: DK-08
**Assignee**: —

### 任务

- [ ] 分享链接 / 协作 / 公开发布
- [ ] **公开发布四重防护**：醒目但非默认 + 二次确认 + Private 工作区禁止 + 发布后顶栏常驻红色标识
- [ ] 协作者管理与评论

### DoD

- [ ] 四重防护全部生效（**缺一不可**——这是整个产品最危险的开关，误触等于把加密内容明文发公网）
- [ ] 评论与批注可追溯

---

## [DK-14] 插件生态（域十五）

**Labels**: `P1`, `area/plugin`, `area/security`
**Milestone**: `M4 · Phase 4 集成与开放`
**Estimate**: 40 人日
**Blocked by**: DK-00
**Assignee**: —

### 任务

- [ ] WASM Component Model + WIT 接口（语言无关）
- [ ] **清除 `invoke` 伪成功**：当前对任意方法名返回 `{"ok":true}`，比返回 Null 更隐蔽
- [ ] 能力注入（Capability）权限模型 L0 / L1 / L2，默认零权限
- [ ] Epoch 中断（100ms / 最大 10 Epoch）
- [ ] **官方先写 10–20 个示范插件**（破解生态冷启动空城）

### DoD

- [ ] 调用未声明函数返回 `NotFound`；调用已声明未实现函数返回 `NotImplemented`
- [ ] 权限在安装前可见（插件市场徽章）
- [ ] 插件越权被拦截并记审计

---

## [DK-15] 移动端（域十六）

**Labels**: `P1`, `area/mobile`, `area/ui`, `area/i18n-a11y`
**Milestone**: `M5 · Phase 5 生态优化`
**Estimate**: 90 人日
**Blocked by**: DK-05M
**Assignee**: —

### 任务

- [ ] **五入口导航重构**：Inbox（收集）/ 今日（执行）/ 笔记（上下文）/ 任务（结构）/ 搜索（回顾）
- [ ] **设置移出主栏**（通过笔记页齿轮进入）——腾出的位置给 Tasks，否则任务过期即消失在视野
- [ ] **移除中央凸起 FAB**（5 入口 + 中心 FAB 造成视觉重心冲突与拇指误触），改右下角
- [ ] 手势：右滑完成 / 左滑计划（滴答清单、Things 3 的肌肉记忆）
- [ ] 安全区与键盘遮挡适配
- [ ] 小组件、分享接收、快捷指令、通知操作
- [ ] Capacitor 升 **v8**（当前 v6）

### DoD

- [ ] VoiceOver / TalkBack 可完整遍历
- [ ] 触摸目标 ≥48×48
- [ ] 动态字体不截断不重叠
- [ ] **界面如实标注"后台同步：关闭"** —— iOS 会挂起后台 socket，接受"回前台 1–3 秒补偿同步"

---

## [DK-16] OCR 与向量：清除静默占位

**Labels**: `P0`, `静默占位`, `area/core`, `type/bug`
**Milestone**: `M1 · Phase 1 核心基石`
**Estimate**: 20 人日
**Blocked by**: DK-00
**Assignee**: —

### 背景

远端存在四类"能编译、能跑、看起来正常"的静默占位，本卡负责其中两类：

| 位置 | 现状 | 危害 |
|---|---|---|
| `l3_domain/ocr_service.rs` | 基于图像 hash 派生 2–3 行伪文本，confidence 0.85+ | **最高**——能进搜索索引、能被用户引用 |
| `l1_infrastructure/vector_db.rs` | LanceDbStore 四方法全 TODO 返回空；lancedb 已从 Cargo.toml 注释掉 | 死代码在假装工作 |

**伪文本比报错危险得多**：报错会被立刻发现并排期，伪文本会被用户当成真实识别结果引用。

### 任务

- [ ] **删除 OCR mock**：无引擎时必须返回 `NotImplemented`，禁止返回伪文本
- [ ] PaddleOCR 通过 feature-gate 提供真实实现（中文主力场景）
- [ ] 向量库：删除 LanceDbStore，或改为 `sqlite-vec` 实现（**不留无依赖的死代码**）
- [ ] 明确 OCR 结果是否进搜索索引（OCR 的主要价值就是图片文字可搜）

### DoD

- [ ] 无 OCR 引擎时调用报错而非返回文本
- [ ] 有引擎时中文准确率 ≥90%
- [ ] 向量检索返回真实相似结果，或该模块被删除且调用方已适配

> 铁律 7 已从"禁止返回 Null"扩展为**"禁止伪成功"**：任何返回成功语义的分支，都必须对应真实发生的副作用。

---

## [DK-17] 备份与灾难恢复

**Labels**: `P1`, `area/storage`, `area/security`
**Milestone**: `M4 · Phase 4 集成与开放`
**Estimate**: 15 人日
**Blocked by**: DK-01
**Assignee**: —

### 任务

- [ ] RPO ≤ 1 小时（现有"每日备份"意味着最坏丢一天，**不可接受**）
- [ ] RTO ≤ 15 分钟
- [ ] **备份完整性每周校验** —— 无演练的备份等于没有备份
- [ ] 异地副本建议

### DoD

- [ ] 从备份恢复全流程可跑通
- [ ] 完整性校验失败会告警
- [ ] 灾难恢复演练记录归档

> **这是本地优先产品的阿喀琉斯之踵。** 云端产品由厂商负责 durability，本地优先产品的 durability 完全由自己的备份策略承担。用户主库与唯一备份同时损坏 = 全损且无处申诉。

---

## [DK-18] 设置与系统（域十四）

**Labels**: `P2`, `area/ui`, `area/desktop`
**Milestone**: `M5 · Phase 5 生态优化`
**Estimate**: 25 人日
**Blocked by**: DK-00
**Assignee**: —

### 任务

- [ ] 设置分组（常规 / 编辑器 / AI / 同步 / 隐私 / 无障碍），可被命令面板检索
- [ ] 用户自定义主题（**仅允许覆盖设计令牌白名单变量**，禁任意 CSS 注入）
- [ ] **自定义快捷键完整方案**（V25 仅一笔带过）
- [ ] 存储管理与占用分析

### DoD

- [ ] 自定义主题必须通过对比度校验，否则拒绝应用并提示
- [ ] 快捷键冲突可检测
- [ ] 无障碍设置不藏在深处

---

## 4. 汇总表

| 卡号 | 标题 | 里程碑 | 优先级 | 人日 | 依赖 |
|---|---|---|---|---|---|
| R-00 | CI 止血 | M0 | **P0** | 1.5–4 | — |
| R-01 | 文档修复 | M0 | P1 | 1–2 | R-00 |
| R-02 | 契约补全 | M0 | **P0** | 8–12 | R-00 |
| R-03 | 缺口补齐 | M0 | P1 | 3–5 | R-02 |
| R-04 | 排期重排 | M0 | P1 | 2 | R-00 |
| DK-00 | 契约层冻结 | M0 | **P0** | 8 | R-02 |
| **DK-01W** | **写入路径统一** | M0 | **P0** | 12–18 | DK-00 |
| **DK-0F** | **前端收敛** | M0 | **P0** | 10–15 | DK-00 |
| DK-05M-V | WebView 输入时序验证 | M0 | **P0** | 5 | R-00 |
| DK-01 | 存储与迁移 | M1 | P1 | 30 | DK-00, DK-01W |
| DK-02 | 组织与结构 | M1 | P1 | 35 | DK-01 |
| DK-03 | 检索 | M1 | P1 | 25 | DK-01 |
| DK-05 | 块编辑器（桌面） | M1 | **P0** | 90 | DK-00, DK-0F |
| DK-05M | 移动端块编辑器 | M1 | **P0** | 65 | DK-00, DK-05M-V |
| DK-16 | OCR 与向量去占位 | M1 | **P0** | 20 | DK-00 |
| DK-04 | 知识网络 | M2 | P2 | 20 | DK-03 |
| DK-06 | 任务与 GTD | M2 | P1 | 60 | DK-01 |
| DK-07 | 安全与加密 | M2 | P1 | 45 | DK-01 |
| DK-08 | 同步 | M2 | P1 | 50 | DK-01 |
| DK-09 | 导入导出与迁移向导 | M3 | P1 | 30 | DK-01 |
| DK-10 | AI 液化与 Agent | M3 | P2 | 55 | DK-00 |
| DK-11 | 画布 | M3 | P2 | 45 | DK-01 |
| DK-12 | 结构化数据 | M3 | P2 | 30 | DK-01 |
| DK-13 | 协作与分享 | M4 | P2 | 30 | DK-08 |
| DK-14 | 插件生态 | M4 | P1 | 40 | DK-00 |
| DK-17 | 备份与灾难恢复 | M4 | P1 | 15 | DK-01 |
| DK-15 | 移动端 | M5 | P1 | 90 | DK-05M |
| DK-18 | 设置与系统 | M5 | P2 | 25 | DK-00 |

**合计 28 张卡**（另加 1 张本汇总说明）。人日区间合计约 840，并行后关键路径约 650，6–8 人团队约 14–16 个月（含 1.5 月缓冲）。

---

## 5. 提交建议

**第一批只提交 R-00。** 理由：五个 job 全红的状态下，其余 27 张卡的验收都无法验证。先让 CI 变绿，再批量提交。

```bash
# 第一批
python3 create_issues.py --filter R-00

# CI 变绿后，第二批
python3 create_issues.py --filter R-01 R-02 R-03 R-04 DK-00 DK-01W DK-0F DK-05M-V

# 契约冻结后，第三批（域卡）
python3 create_issues.py
```

---

## 6. V26.1 审阅补充卡（2026-09-10 竞品深度调研审阅新增）

> 来源：《Aurora Note V26.0 架构与产品设计规格审阅报告》（16 款竞品对照）。五张卡均为规格既有卡池的补强，不改变 R-00 优先的铁律。

## [RV-01] 性能基准对测三件套：把「性能上限」变成可引用的数据

**Labels**: `P2`, `type/chore`, `area/arch`
**Milestone**: `M1 · Phase 1 核心基石`
**Estimate**: 3–5 人日
**Blocked by**: R-00
**Assignee**: —

### 背景

V26 定义了冷启动 <2s、10k 笔记检索 <200ms 等指标，但没有与竞品的对测基准。竞品调研显示：Electron 系（Joplin 大库卡顿是社区长期反馈）与 Rust/Tauri 系存在真实代差，但该优势目前无法用数据引用。对照对象：Obsidian、思源笔记。

### 任务

- [ ] 建立三件套基准：冷启动耗时 / 万级笔记全文检索 P50+P99 / 双端同步收敛延迟
- [ ] 数据集生成器（1k/5k/10k 笔记，含长文档与附件引用）
- [ ] CI 中每周跑一次 Aurora 侧基准并留档趋势
- [ ] 产出对照表（Aurora vs Obsidian vs 思源），写入 docs/

### DoD

- [ ] 三件套可重复运行，数据落档
- [ ] v26 性能指标逐项有实测值而非目标值

---

## [RV-02] 插件 TS SDK：WASM 之上必须提供低门槛层

**Labels**: `P2`, `type/feat`, `area/plugin`
**Milestone**: `M4 · Phase 4 集成与开放`
**Estimate**: 8–12 人日
**Blocked by**: DK-14
**Assignee**: —

### 背景

竞品调研结论：Obsidian 用 JS 插件低门槛换来 2000+ 插件生态；Aurora 的 wasmtime WASM 沙箱更安全更快，但直接写 WASM 的开发者门槛高一个数量级。若不提供 SDK，插件生态冷启动大概率失败。

### 任务

- [ ] TypeScript SDK：类型化 Plugin API（覆盖 CoreAPI 契约子集）
- [ ] 脚手架 CLI（init/build/pack 一条命令）
- [ ] 示例插件 ≥3 个（命令/视图/处理器各一）
- [ ] 开发者文档（对标 Obsidian Community 目录的引导页）

### DoD

- [ ] 一个仅会 TS 的开发者可在 30 分钟内发布可安装插件
- [ ] SDK 契约与 CoreAPI 契约同源生成，禁止手写镜像

---

## [RV-03] 导入器前置：DK-09 从 M3 提到 M2

**Labels**: `P1`, `type/chore`, `area/arch`
**Milestone**: `M2 · Phase 2 任务与安全`
**Estimate**: 0.5（排期调整）+ DK-09 原预算
**Blocked by**: R-00
**Assignee**: —

### 背景

竞品调研结论：新笔记软件的获客第一入口是「从旧软件无痛迁移」（印象/为知 ENEX、有道导出、Markdown 目录）。V26 将 DK-09（导入导出与迁移向导）排在 M3，晚于用户做出弃用决策的关键窗口。本卡只调整排期，不新增实现预算：ENEX/Markdown 目录两格式随 M2 交付，其余格式留在 M3。

### 任务

- [ ] DK-09 拆分：M2 交付 ENEX 解析 + Markdown 目录导入；M3 交付其余格式与迁移向导
- [ ] 排期表同步更新

### DoD

- [ ] 从印象笔记导出的 ENEX 可导入且块结构无损
- [ ] 里程碑表与本清单一致

---

## [RV-04] NoteOperator 调研补录：竞品清单缺口的诚实闭环

**Labels**: `P2`, `type/docs`, `area/arch`
**Milestone**: `M0 · Phase 0 止血与收敛`
**Estimate**: 0.5 人日
**Blocked by**: —
**Assignee**: —

### 背景

审阅调研中 NoteOperator 未检索到任何可靠公开信息（截至 2026-09-10），疑似名称有误或为极小众/未公开发布产品。对照清单应保持诚实：无法证实的产品不应出现在结论矩阵中。

### 任务

- [x] 向需求方核实准确名称（或确认是否 EdgeEver/NoteGen 的别名）— V26 审阅报告已如实标注「未检索到可靠公开信息（截至 2026-09）」并记录多轮检索过程（GitHub/中文社区/产品名变体）
- [x] 若确有其品：补录技术栈与功能行；若没有：从对照清单移除并记录原因 — 处置：保留条目但维持「未证实」标注（有来源说明非无来源条目），待需求方核实后处置

### DoD

- [x] 竞品对照清单中不存在无来源条目 — NoteOperator 条目带显式检索记录标注

---

## [RV-05] 移动端块编辑器降级备胎：DK-05M-V 的 No-Go 预案

**Labels**: `P1`, `type/chore`, `area/mobile`, `area/editor`
**Milestone**: `M1 · Phase 1 核心基石`
**Estimate**: 1–2 人日（方案定义；实现预算仅 No-Go 时申请）
**Blocked by**: DK-05M-V
**Assignee**: —

### 背景

移动端块编辑器（DK-05M，70 人日）是全项目技术密度最高模块，其前置验证 DK-05M-V（WebView 输入时序）可能出现 No-Go（焦点/滚动/组合输入不可控行为）。竞品对照：思源/AppFlowy 用原生渲染规避此问题，Obsidian 用 5 年打磨换稳定。若 Go 无预案，70 人日投入将悬空。

### 任务

- [ ] 定义降级方案 A：纯 Markdown textarea 增强版（复用 V23 FallbackToolbar 经验：行前缀/选区包裹/光标 rAF 恢复）
- [ ] 定义降级方案 B：编辑器只读+桌面写（移动端检索/浏览优先，编辑降级）
- [ ] 明确两方案的体验取舍与切换开关设计
- [ ] 在 DK-05M-V 结论为 No-Go 时作为唯一立项依据

### DoD

- [ ] 两方案有明确的 DoD 与体验边界，无需二次设计即可开工
