# WF-Delta 任务书：DK-11 画布（apps/desktop 领地）

> 派发对象：Delta 工作流 agent（独立上下文可执行）
> 生成：2026-09-19 · Repo: Lizeyun8501/Aurora · 基线 commit: 58a2a3c（main, CI passing）

## 你的领地（可修改）

- `apps/desktop/**` —— **全部独占**（Tauri 桌面端，React/TS）

## 禁改（改动会被 Alpha 集成时打回）

- `crates/**`（画布后端如需 Rust 支撑 → 写 `delta-request-<主题>.md` 提给 Alpha）
- `apps/mobile/**`、`apps/web/**`、`apps/extension/**`

## 背景

- DK-11 卡（45 人日，M3 P2）：画布。issue 清单 §[DK-11]
- desktop 端技术栈与 `apps/mobile` 同构（React + TS），参考 `apps/mobile/src/MobileApp.tsx` 的组件与 styles 组织；数据面走 Aurora 服务接口
- 无限画布白板：节点（笔记卡/便签/连线）自由摆放

## 第一切片（5–8 人日）：画布骨架

1. 新路由/页签 `CanvasView`（挂进 desktop 现有导航，不动其他视图文件）
2. 画布数据模型（TS）：
   ```ts
   interface CanvasNode { id: string; kind: 'note'|'sticky'|'group';
     x: number; y: number; w: number; h: number; refId?: string; color?: string; }
   interface CanvasEdge { id: string; from: string; to: string; label?: string; }
   interface CanvasDoc { id: string; title: string; nodes: CanvasNode[]; edges: CanvasEdge[]; }
   ```
3. 交互最小集：节点拖拽移动、框选/单选、连线创建（节点边缘拖出）、删除
4. 持久化：CanvasDoc JSON 存本地（与移动端 `aurora.tasks` localStorage 模式一致），后续接 Aurora 存储由 Alpha 冻结接口
5. 缩放平移（Ctrl+滚轮 / 空格+拖拽）

## 验收标准

- `npm run typecheck` + `npm run build` 全绿（apps/desktop）
- 交互 demo：创建 3 节点 → 连线 → 拖动 → 刷新页面布局保持
- 不破坏现有视图（全量回归：笔记列表/编辑器正常）

## 工作流规约（全 WF 通用）

1. 分支：`wf/delta-canvas`，每日 rebase main
2. 提交：`feat(DK-11): ...`
3. 本地验证铁律：typecheck + build；clippy 不适用（纯 TS）
4. 集成：每日末推分支，Alpha 次日 merge
5. 需要后端能力：写 `delta-request-<主题>.md` 提给 Alpha
