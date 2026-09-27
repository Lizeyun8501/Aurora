# 重派任务书 — DK-11 画布（原 Delta，8 天零响应，收回）

> 领受对象：**Alpha**（下一会话开工；本会话上下文超载不做实现）
> 生成：2026-09-27 · 基线 f23490a · 用户指令「同时处理 DK-10 和 DK-11」
> 原任务书 issues/wf/delta-DK11-画布.md（09-19）作废归档。

## 范围（清单卡原文锚点，issues/v26_issue清单.md §DK-11）

1. 无限画布 + LOD 分层渲染 + 视口裁剪；
2. 节点内容引用 `content_ref`（**不内嵌正文**），布局数据单独立文档；
3. 三种布局：自由 / 思维导图树形 / 网格；
4. 导出 SVG / PNG / JSON / Markdown；
5. Canvas2D 起步，WebGL 置 Phase 5。

DoD：1000 节点渲染 ≥30fps（桌面）/≥25fps（移动）；10000 节点加载 <2s；画布数据可 CRDT 协同编辑。

## 关键约束（原 Delta 任务书有效部分继承）

- 领地 `apps/desktop/**`；Rust 侧如需支撑走 `alpha-request-<主题>.md` 提案，不直改 crates/**；
- **CanvasEditor 已在 DK-0F editor-uplift 中删除**（TipTap 系退役）——本卡画布为独立实现（Canvas2D 起步），不得复活 TipTap 栈；
- 节点 content_ref 对齐 DK-0F 上移后的 schema（block 语义以 shared/ui-components editors 为准）。

## 第一切片（Alpha，5–8 人日）

1. `CanvasView` 新路由/页签挂进 desktop 导航；
2. TS 画布数据模型：节点 {id, content_ref, x, y, w, h, kind} + 连线 {from, to, label?}；
3. Canvas2D 渲染骨架：视口变换（pan/zoom）+ 节点绘制 + 选中态；
4. 布局数据独立文档（不入笔记正文）+ 本地持久化走既有存储接口；
5. 验证脚本 `dk11_verify.js`：渲染冒烟 + pan/zoom 断言 + 1000 节点 fps 采样。

— Alpha 重派 2026-09-27
