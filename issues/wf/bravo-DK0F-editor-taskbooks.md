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

— Alpha 2026-09-24（原件）/ 2026-09-25（迁移重发）
