# Alpha 任务书：删除 apps/web（DK-0F 任务 1 执行确认）

> 发起：Bravo · 2026-09-25（对 bravo-DK0F-editor-taskbooks.md 申请 3 的裁决产出）
> 执行：Alpha · 状态：**待取件**
> 依据：ADR-005 D1（保 desktop、删 web；「方案 A」作废）
> 性质：独立先行小卡，与编辑器上移（editor-uplift）零文件交集，可并行。

## 1. 背景与现状

- `apps/web` 为 V19 零迭代遗留：TipTap 栈（`@tiptap/react` / `@tiptap/starter-kit` / `@tiptap/pm` ^2.10.0）无 CRDT；
- Tauri `frontendDist` 已指向 `apps/desktop` 自身产物（`../dist`），web 无运行时引用；
- `.github/workflows/ci.yml` **无 apps/web 直接字样引用**（2026-09-25 复核）——但 lockfile / workspace 配置 / tsconfig 路径引用需执行时逐一核对。

## 2. 交付清单

1. 删除 `apps/web/` 整目录（git 历史可溯）；
2. 依赖与配置残留清理：
   - root lockfile（pnpm-lock.yaml / package-lock.json 按实际）中 web 包条目移除；
   - workspace 配置（root package.json `workspaces` / pnpm-workspace.yaml 若存在）核对；
   - `tsconfig.base.json` 及各 tsconfig 中 web 路径引用核对清除；
   - vite/vitest 配置中 web 残留（`apps/web` 字样全仓 rg 清零）；
3. CI workflow 核对：删除后全绿（web 相关 job/step 若隐藏引用一并清除）；
4. **不触碰** `shared/ui-components` 的 TipTap 依赖（editor-uplift 卡收尾统一处理）。

## 3. 边界

| 项 | 归属 |
|---|---|
| apps/web 目录 + root 配置/lockfile/CI 清理 | 本卡（Alpha） |
| `shared/ui-components` TipTap 依赖删除 | editor-uplift 卡 |
| desktop / mobile 任何文件 | 不在本卡 |

## 4. 验收门槛

- `apps/web` 不存在；全仓 rg `apps/web` 仅剩 git 历史与 ADR/文档引用；
- desktop + mobile 双端 tsc + vite build 绿（门禁不受影响）；
- CI 全绿（推送后 Run 通过）；
- Rust workspace 零改动（`git diff --stat` 仅删 web + 配置文件）。

## 5. 提交约定

- 分支：`wf/alpha-dk0f-drop-web`（自 origin/main 最新切出）；
- commit 前缀 `chore(DK-0F):`；单 commit 语义完整；
- push 前自查 `origin/main..HEAD` 只含本卡范围。

— Bravo 2026-09-25
