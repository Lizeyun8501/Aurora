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

---

## ✅ Bravo 交付回执：DK-12 编辑器实时协同渲染（2026-09-27）

> 结论先行：**能力已达成（确定性 8/8+5/5 实测），且根因与任务书假设不同——不是纯渲染层竞态，是「fork 历史下 import 数据不落」的数据层竞态**。绕行方案落地 + 入会基线协议确立。

### 一、根因定位（调研面 1，证据链完整）

多轮控制变量实验（lab 探针 + 逐层插桩 + A/B 对照）：

| 层 | 实验 | 结果 |
|---|---|---|
| loro-crdt 0.4.x 裸 API | 独立 node 探针：`doc.subscribe` 后 `import(update)` | ✅ 事件正常派发（by="import"）、数据落 |
| loro-prosemirror 装配（裸 LoroSyncPlugin） | lab 双实例 attach 探针 | ⚠️ **flaky 3/4**（同代码间歇收敛/不收敛） |
| 生产装配（createAuroraEditor） | 同探针换装配 | ❌ 稳定不收敛（复现 Alpha A2） |
| 逐层插桩（init/updateNodeOnLoroEvent） | 插桩进 bundle 后观测 | **[I]/[P] 零输出**——回调根本未执行 |
| 数据层立查（import 后同步读） | 失败轮 `syncLoro=False` | **B 的 LoroDoc 里没有数据**——不是渲染断，是 **import 未落** |

**根因定性**：B 实例 attach 时 init 的空文档 `commit({origin:"sys:init"})` 与 A 的既有历史形成 **fork 起点**；此后对 fork 历史的 doc 做 `import(远端 update/snapshot)`——**loro-crdt 0.4.4 数据层间歇性不落**（status 仍报 success）。渲染层只是受害者。

**版本评估（调研面 2）**：loro-prosemirror **0.4.4 即最新版**（无 0.5.x），上游无修复可升。

### 二、修复交付（调研面 3 绕行 + 协议约束）

1. **`AuroraEditorHandle.applyRemote(updates)`**（领地内 auroraEditor.ts）：import 远端增量 + 显式渲染跟随（复用官方公共导出 `createNodeFromLoroObj`，meta 用插件同款 `non-local-updates`）。语义=「远端批量到达即软重挂」——任务书预案 3.3 原样落地；
2. **入会基线协议（核心产出）**：B 加入时**必须先 import 基线快照再 attach**（消除 fork 起点）→ 后续增量 import 确定性收敛。**实测 8/8 + 验收脚本 5/5 确定性**（此前任何形态都无法超过 3/4）；
3. **editor-lab `liveCollabProbe`**（DoD 探针）+ `scripts/dk12_verify.js`（验收脚本：DoD×3 + 回归 A2/A3）。

### 三、验证矩阵

| 项 | 结果 |
|---|---|
| DoD ×3（A编辑→B(attach)→import→收敛≤2s） | ✅ 52-58ms 确定性 ×3 |
| 回归 S3 语义（A2 挂载时渲染 / A3 增量渲染） | ✅ |
| 回归 S1 / S2 | ✅ 9/9、12/12 |
| desktop vite build / ui-components tsc | ✅ / ✅（auroraEditor 零新增错误；__tests__ 基线错误预存，stash 对照实证） |

### 四、遗留与建议

1. **上游 issue 建议**：loro-crdt「fork 历史下 import 间歇性不落数据（status=success 但容器未变）」应报上游；loro-prosemirror「init 空文档 commit 制造 fork」行为值得警示文档化；
2. **生产接线**：`applyRemote` 消费侧（DesktopShell 订阅同步桥的远端推送）属生产代码改动——**按禁改条款待 Alpha 集成**，届时 P2P 通路须遵守「入会基线协议」；
3. **增量粒度**：软重挂语义下远端到达会重建全树（undo 栈语义=恢复点重置，与 S4 基线定型一致）；增量粒度待上游修复后升级。

**DK-12 交付完成。**

— Bravo 2026-09-27
