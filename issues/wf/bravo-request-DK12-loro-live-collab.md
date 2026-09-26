# WF-Bravo 任务书：DK-12 编辑器实时协同渲染（loro-prosemirror 远端更新通路）

> 派发对象：Bravo 工作流 agent
> 生成：2026-09-27 · 基线 commit: f722f3f（main，DK-05 收官 + R1R2R3 闭环 + 对向审核通过）
> 来源：DK-05 S4-5 上游备忘（Alpha 回执）——独立卡承诺落地

## 1. 背景（实测缺陷定性）

loro-prosemirror **0.4.4** 实测：已 attach 的 EditorView 对后续 `doc.import(updates)`（远端增量更新）**不渲染**。

- 证据链：`node_modules/loro-prosemirror/dist/index.js` `updateNodeOnLoroEvent`（L695）——`event.by === "import"` 分支存在且会 `view.dispatch(tr)` 全量替换，但实测未触发（A2 探针：loro 层 ImportStatus success、数据在、PM state 恒空）；
- 疑点：init（L658）的订阅选择——`state.containerId ? doc.getContainerById(...).subscribe : doc.subscribe`，import 事件可能未到达已选订阅，或事件形参不含 `by: "import"`；
- 生产语义不受影响：DK-05 主链路是「数据先收敛 → 挂载时渲染」（初始同步），已验证 6/6。

## 2. 目标

编辑态下远端更新到达后 **view 实时渲染跟随**（P2P 协同渲染前置能力）。

## 3. 调研面（按序）

1. **根因定位**：0.4.4 源码走读——import 事件是否触发 `doc.subscribe`（对比 loro-crdt 的 subscribe 语义：本地事务 vs 远程 import 的事件形参差异）；
2. **版本评估**：loro-prosemirror 0.5.x+（如有）是否已修 remote-updates 推送；升级破坏面（createAuroraEditor 插件栈 API 兼容）；
3. **绕行方案**（若上游无解）：EditAuroraEditor/handle 增 `applyRemote(updates)`——import 后手动重建 view state（复用初始同步路径），语义=「远端批量到达即软重挂」，渲染正确性优先、增量粒度放弃。

## 4. 验收（DoD）

- 双实例探针（editor-lab 扩展）：A 编辑 → B **已 attach** → import → B 渲染收敛（轮询 ≤2s，条件式等待）；
- 回归：S3 A1 快照往返 / A2 双实例（挂载时渲染）/ dk05_s3_verify.js 6/6 不破坏；
- 多容器混合场景不回归（内核容器 + "doc" 容器共存语义）。

## 5. 领地与禁改

- **领地**：`shared/ui-components/src/editors/**`（评估升级/绕行实现）、editor-lab 探针扩展；
- **禁改**：`crates/**`（不动内核）、apps/desktop 生产代码（EditAuroraEditor 消费侧改动需回执声明并经 Alpha 集成）；
- 依赖升级（loro-prosemirror/loro-crdt）需回执声明版本与双端 lockfile 同步。

— Alpha 派发 2026-09-27
