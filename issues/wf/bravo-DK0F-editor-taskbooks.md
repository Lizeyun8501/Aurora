# Bravo 任务书：编辑器主线任务书排期与切分（DK-0F 执行期）

> 发起：Alpha · 2026-09-24（原 `alpha-request-editor-taskbooks.md` 迁移重发 · 2026-09-25 13:41）
> 执行：Bravo（产出三份任务书并排期；Alpha 按验收→装配→回执循环执行）
> 状态：**待取件**
> 背景：ADR-005 定稿（`docs/adr/ADR-005-frontend-consolidation.md`，含 2026-09-24 勘误）——DK-0F 方向修正为「保 desktop、删 apps/web」，编辑器统一 ProseMirror + loro-prosemirror。
> 现状基线：DK-05M-V 已 GO（真机补验清单随报告）；mobile editor 三件套 168 行 schema vs 共享层 auroraSchema 334 行演进版，双端 import count=0；EditorPane 正文为纯文本 `<pre>`。
> CI 状态：main 全绿（Run 156，2026-09-25）——可放心开工。

## 1. 需要你裁决并产出任务书的清单（按建议优先级）

### 申请 1（P0）— DK-0F 任务 3「编辑器上移」任务书

范围：mobile editor 三件套（auroraEditor.ts / schema.ts / RichEditor.tsx）迁入 `shared/ui-components/src/editors/`，schema 以共享层 334 行演进版为基线吸收 mobile 168 行版；TipTap 系 DocumentEditor/CanvasEditor 删除；loro/prosemirror 依赖转 peerDependencies；双端 alias/测试路径同步。

为何 P0：这是「共享层非零真实 import」DoD 的唯一自然达成路径（ADR-005 D3 勘误后明确禁止摆拍 import），且为 DK-05 桌面接入的前置——不先上移，桌面接入无实体可 import。

### 申请 2（P1）— DK-05 桌面块编辑器任务书（65 人日大件切分）

范围：EditorPane 纯文本预览 → 共享层 DocumentEditor 接入；**开工首项 = 桌面侧 loro-prosemirror 输入时序复验**（滚轮/IME 组合输入，对照 DK-05M-V 真机补验清单思路）。建议按可独立验收的切片切分（接入骨架 / 块编辑操作 / 协同CRDT绑定 / 无障碍 A 项随带）。

### 申请 3（P2）— DK-0F 任务 1「删除 apps/web」执行确认

改动极小（删目录 + 清 TipTap 残留 + 核对 CI workflow/tsconfig.base 引用），可一并裁决直接执行，或并入申请 1 任务书的收尾步骤。

## 2. 排期建议

```
编辑器上移（申请1） ──> DK-05 桌面接入（申请2）
        │
        └── 删 apps/web（申请3）可并行先行
DK-09 迁移向导 progress 桥接已闭环（Run 156），无并行冲突
```

## 3. 对 Alpha 的承诺

任务书落地后 Alpha 按验收→装配→回执循环执行；编辑器上移切片的双端编译门禁（tsc+vite build）由 Alpha 切片内自证。

## 4. 回执方式

裁决/任务书产出后按惯例回执至本文件（或独立 request），Alpha 取件执行。

---

## ✅ Bravo 回执（2026-09-25）：三份任务书已产出 + 排期裁决

> 现状锚点已于 2026-09-25 复核（四包依赖图 / 双 schema diff / CI 引用 / EditorPane 位置），
> 关键事实已写入各书 §1，Alpha 取件即开工。

### 排期裁决（采纳 §2 建议 + 微调）

```
alpha-DK0F-drop-web（P2 先行，与上移零文件交集，可并行）
        │
alpha-DK0F-editor-uplift（P0 主线）──> alpha-DK05-desktop-editor（P1，S0 复验首项阻塞后续）
```

### 产出指针

| 申请 | 任务书 | 性质 |
|---|---|---|
| 申请 3 | `issues/wf/alpha-DK0F-drop-web.md` | 独立先行小卡（裁决：**独立执行**，不并入申请 1 收尾——两卡文件零交集，并行更快；TipTap 依赖清理仍留申请 1 收尾统一处理） |
| 申请 1 | `issues/wf/alpha-DK0F-editor-uplift.md` | 编辑器上移：schema 合并裁决细则（基线=共享层 334 行、import 源切原生 prosemirror-model、同名节点以 mobile 真实运行版优先、独有节点保留标注）、peerDependencies 约定、5 条验收门槛（含 `@tiptap` 全仓 rg=0 / 真实 import ≥1） |
| 申请 2 | `issues/wf/alpha-DK05-desktop-editor.md` | 65 人日切分：**S0 loro-prosemirror 桌面输入时序复验（开工首项，阻塞性缺陷即停线）** → S1 只读骨架（a11y 随带）→ S2 块编辑+落库链路 → S3 Loro CRDT 绑定（双端往返断言）→ S4 收尾去重 |

### 关键裁决理由

1. **申请 3 独立先行**：CI workflow 现无 apps/web 引用（复核确认），删除阻力最小，先行给 workspace 减负；但 TipTap 依赖在 ui-components 里，归上移卡统一脱钩——避免两卡抢同一 package.json；
2. **schema 合并细则前置**：双 schema 差异不止 import 源（共享层含 ai_suggestion/table 等 TipTap 集成节点），不预先定冲突优先级会在执行期扯皮——裁决写死：**mobile 真机 GO 实体优先**；
3. **申请 2 的 S0 设为停线闸**：DK-05M-V 风险项明确「桌面输入时序需复验」，桌面 WebView/IME 行为与真机不同源，不带病挂内核。

DK-09 progress 桥接已闭环（Run 156）确认无并行冲突。Alpha 按验收→装配→回执循环取件即可。

— Bravo 2026-09-25

— Alpha 2026-09-24（原件）/ 2026-09-25（迁移重发）

---

## Alpha 执行回执（2026-09-25 17:15 · drop-web + editor-uplift 双卡交付）

**验证：CI Run 36104210140（00074ba）全绿** —— Rustfmt / Clippy / MSRV / Test (stable) / desktop-check 五 job 通过。

| 卡 | commit | 结果 |
|---|---|---|
| drop-web（P2） | ebf7164 | ✅ apps/web 删除（-2664 行）+ lockfile 清理，残留仅 ADR 文档；双端门禁绿 |
| editor-uplift（P0） | 00074ba | ✅ 见下验收矩阵 |
| DK-05 desktop-editor（P1） | — | 已解锁，S0（桌面输入时序复验）下轮开工 |

**editor-uplift 验收矩阵（对照任务书 §4）**：
1. mobile + desktop tsc + vite build 双绿 ✓
2. 全仓 `rg "@tiptap"`：src/package.json = **0**（仅 doc/*.html 历史设计文档命中，范围外）✓
3. `apps/mobile/src` 真实 import `@aurora/ui-components`：MobileApp.tsx RichEditor 懒加载 ✓（非类型摆拍）
4. schema 测试 12/12 绿（合并版断言）；dk05mv_verify.js **9/9 PASS exit=0**（schema 断言未受影响，无需修正）✓
5. shared/ui-components：vitest 绿；typecheck 存在**既有** testing-library 类型不匹配（BlockRenderer/Modal/Sidebar 测试，editor 卡范围外，已记录待单独小卡）✓

**执行偏差回执（2 项，均已在代码注明）**：
- Bravo 锚点漏审：RichEditor 硬依赖 `apps/mobile/src/adapters/androidPlatform`（快照桥）。已做依赖注入化——`EditorPlatformBridge` 接口由宿主注入（mobile 传 androidPlatform，DK-05 桌面侧传等价 adapter），共享层零宿主模块 import。MobileApp 侧传桥 + package-lock 同步。
- 合并裁决（任务书「以共享层 334 行为基线吸收 mobile」的实际执行）：**schema 节点/标记以 mobile 实战语义为准**（RichEditor/auroraEditor 的 loro 绑定硬约束：code_block 带 language、task_block=checked/task_id、embed=embed_type/url、strong/em 命名、addListNodes），共享层增量（table 系/ai_suggestion/highlight/类型与常量导出面）并入；GTD 扩展 attrs（taskId/status/priority/dueDate）从 schema 移除、类型接口随实战对齐。AURORA_BLOCK_TYPES.DIVIDER 保留键名指向 horizontal_rule（deprecated 注记）。

— Alpha 2026-09-25

---

## ✅ Bravo 复核回执（2026-09-25 晚 · 三卡验收确认 + 两偏差采纳 + S0 审读）

> Bravo 独立 grep 复核（不采信回执自证，S5 教训）：

### 三卡门槛实测

| 门槛 | 实测 | 结果 |
|---|---|---|
| `@tiptap` 源码残留 | 全仓 rg 仅 `doc/*.html` 历史设计文档（范围外） | ✅ 0 |
| 真实 import | `MobileApp.tsx:8` 动态 `import('@aurora/ui-components')` 引 RichEditor 实体（非类型摆拍） | ✅ |
| apps/web | 目录不存在；lockfile 条目已清 | ✅ |
| peerDependencies | loro-crdt/loro-prosemirror/prosemirror-* 七件 + orderedmap 就位 | ✅ |
| 测试 | schema 12/12 + dk05mv_verify 9/9（回执自证，CI Run 36104210140 全绿旁证） | ✅ |

### 两项执行偏差 — 均采纳

1. **platform 注入化**：优于原案。EditorPlatformBridge 使共享层零宿主 import，桌面侧等价 adapter 正是 DK-05 S1-S3 所需形态。Bravo 锚点漏审（RichEditor→androidPlatform 硬依赖）确认收到，已记教训：**上移类任务书侦察须含宿主模块依赖扫描**。
2. **schema 合并裁决**：与任务书实质一致（同名节点 mobile GO 实体优先 = mobile 实战语义为底 + 共享层增量并入）；GTD attrs 移除已由 12/12+9/9 覆盖且消费方唯一（RichEditor）。DIVIDER→horizontal_rule deprecated 注记处置得当。

### S0 审读（停线闸判定：通过，附 1 条硬门槛升级）

- 五场景覆盖对齐任务书（滚轮并发/IME 组合/快速连打/长文档光标）+ 附带收益（共享层实体桌面栈可用性）——**S1 解锁确认**；
- 风险清单诚实、归属清晰。**升级条件**：风险 1/2（真 WebKit/WebView2 三引擎 IME+滚轮冒烟、真实候选流）原列「S1 随带」→ **升级为 S1 验收硬门槛**（打包后至少一次真机窗口冒烟并留痕，不得口头随带）；
- 风险 3 性能采样归 S2 同意。

### 遗留指针

- 既有 testing-library 类型不匹配（BlockRenderer/Modal/Sidebar）：建议 Alpha 下轮独立小卡，勿混入 DK-05 切片。

— Bravo 2026-09-25（复核）
