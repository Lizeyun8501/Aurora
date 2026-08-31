# ADR-001: 前端框架统一决策（React 18 + Zustand）

- 状态: 已接受（Accepted）
- 日期: 2026-08-30
- 决策人: 架构评审组
- 依据: V20 §2.2 GAP-02 / §3.1 / §6.1 Phase 0 P0-2
- 替代方案: Svelte 5 反向迁移（否决）

## 背景与语境

V19 §5.3 原定 Svelte 5，但移动端实际落地为 React 18 + Zustand
（apps/mobile，~1.5 万行 TSX），桌面端无框架无打包器。双端栈分叉导致
"前端 100% 共享与统一 CoreAPI"失效（V20 GAP-02，P0）。

## 决策

**统一到 React 18 + Zustand**，桌面端以 Vite + Tauri v2 接入同一技术栈：

1. 移动端资产（组件/编辑器/状态层）直接复用，UI 复用率最大化
2. CoreAPI 经薄适配层（移动端 JNI bridge / 桌面端 Tauri invoke）双端注入
3. 共享层沉淀于 `packages/shared`（类型 + CoreAPI 契约 + 纯逻辑），
   由两端 workspace 引用（npm workspaces）
4. V19 中 Svelte 5 的记述转为历史决策记录，不再作为目标

## 后果

- 正面: 消除双端分叉风险；编辑器（ProseMirror）生态 React 绑定成熟；
  招聘与维护成本降低
- 负面: bundle 体积相对 Svelte 偏大（~40KB gzip），以 code-splitting 缓解
- 治理: `@aurora/ui-components` 与 `@aurora/shared-types` 随 Phase 0 建立，
  UI 复用率纳入 §6.2 度量（目标 ≥70%）

## 复核

Phase 0 结束时以「桌面端可运行 + 双端编译门禁绿」为退出条件复核本 ADR。
