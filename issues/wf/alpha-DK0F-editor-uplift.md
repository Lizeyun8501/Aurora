# Alpha 任务书：编辑器上移（DK-0F 任务 3 · 共享层非零真实 import）

> 发起：Bravo · 2026-09-25（对 bravo-DK0F-editor-taskbooks.md 申请 1 的裁决产出）
> 执行：Alpha · 状态：**待取件**
> 依据：ADR-005 D2（编辑器统一 ProseMirror + loro-prosemirror）+ D3 勘误（零 import 的解法是产生真实引用，禁止摆拍）
> 性质：P0 主线，DK-05 桌面接入的前置（不先上移，桌面无实体可 import）。

## 1. 背景与现状锚点（2026-09-25 复核）

| 实体 | 位置 | 行数 | 栈 |
|---|---|---|---|
| mobile 三件套 | `apps/mobile/src/editor/` auroraEditor.ts / schema.ts / RichEditor.tsx | 191 / 168 / 523 | 原生 prosemirror-* + loro-prosemirror 0.4.4 + loro-crdt 1.14.1 |
| 共享层 schema | `shared/ui-components/src/schema/auroraSchema.ts` | 334 | **`@tiptap/pm/model` import（TipTap re-export）** |
| TipTap 系 editors | `shared/ui-components/src/editors/` DocumentEditor.tsx / CanvasEditor.tsx | 249 / 74 | TipTap，双端 import count=0 |
| ui-components 依赖 | `shared/ui-components/package.json` :15-17 | — | @tiptap/react / starter-kit / pm |
| mobile 依赖 | `apps/mobile/package.json` :16-25 | — | loro-crdt / loro-prosemirror / prosemirror-* 七件 |

**关键事实**：两个 schema 的差异不止 import 源——共享层 334 行含 `ai_suggestion` / `table` 等 TipTap 集成注释与节点；mobile 168 行是 DK-05M-V 真机 GO 的**真实运行实体**（节点: doc/paragraph/heading/code_block/task_block/embed/blockquote/lists/hr）。

## 2. 交付清单

### 2.1 schema 合并（以共享层为基线吸收 mobile 版）

- 产出单文件 `shared/ui-components/src/schema/auroraSchema.ts`（重写）：
  - **import 源切换**：`@tiptap/pm/model` → 原生 `prosemirror-model`（脱 TipTap，D2 统一栈）；
  - 节点/标记集 = 两版**并集**；同名节点 toDOM/parseDOM 冲突时**以 mobile 真实运行版优先**（GO 实体）；
  - 共享层独有节点（ai_suggestion / table 等）保留并注释标注「desktop 未接，DK-05 排期」；
  - 合并对照表（节点 × 来源 × 冲突裁决）写入 commit message 或附注，防静默丢失。

### 2.2 三件套上移

- `apps/mobile/src/editor/` → `shared/ui-components/src/editors/aurora/`：
  - auroraEditor.ts → `editors/aurora/auroraEditor.ts`（import 改指合并后 schema）；
  - RichEditor.tsx → `editors/aurora/RichEditor.tsx`；
  - 旧 `apps/mobile/src/editor/` 删除；
- mobile 侧改为 **import 共享层**（`@aurora/ui-components`，通道已就位）——达成共享层第一处真实 import（DoD）；
- mobile vite alias / vitest 配置 / 测试文件路径同步（DK-05M-V 的 `vite.config.editorlab.ts` 与 `scripts/dk05mv_verify.js` 路径引用一并核对）。

### 2.3 TipTap 全系清除（共享层）

- `editors/DocumentEditor.tsx` / `CanvasEditor.tsx`（TipTap 系）删除；
- `shared/ui-components/package.json`：@tiptap/* 三依赖删除；`loro-crdt` / `loro-prosemirror` / `prosemirror-*` 以 **peerDependencies** 声明（宿主提供，D2 约定）；`orderedmap` 等直接依赖归 dependencies；
- 全仓 rg `@tiptap` 归零（apps/web 已由 drop-web 卡删除；若其未先行，本卡不得动 apps/web）。

## 3. 边界

| 项 | 归属 |
|---|---|
| schema 合并 + 三件套上移 + TipTap 清除 + mobile 改 import | 本卡（Alpha） |
| desktop EditorPane 接入 | DK-05 卡（另书） |
| apps/web 删除 | drop-web 卡（可并行先行） |
| blocks/ 块渲染器 | 不动（ADR-005 明示不受影响） |

## 4. 验收门槛（双端编译门禁由 Alpha 切片内自证，§3 承诺）

1. `apps/mobile` + `apps/desktop` tsc + vite build 双绿；
2. 全仓 `rg -c "@tiptap"` = **0**（package.json / src 全域）；
3. `rg "from '@aurora/ui-components'"` 在 `apps/mobile/src` **≥ 1 处真实 import**（编辑器实体，非类型摆拍）；
4. mobile editor 现有测试全绿（路径迁移后）；`dk05mv_verify.js` 退出码 0（若其 schema 断言受合并影响，修正断言并注明）；
5. `shared/ui-components` 自身 lint/build 绿（peerDependencies 声明后宿主外构建不假死）。

## 5. 提交约定

- 分支：`wf/alpha-dk0f-editor-uplift`（自 origin/main 最新切出）；
- commit 前缀 `feat(DK-0F):` / `refactor(DK-0F):`；schema 合并、上移、TipTap 清除可拆 2-3 个语义完整 commit；
- push 前自查 `origin/main..HEAD` 只含 shared/ui-components + apps/mobile + 配置文件。

— Bravo 2026-09-25
