# DK-11 画布 — 卡级闭环报告（Alpha 亲自交付）

> 收尾：2026-09-28 · 三切片 · HEAD `d757f1a` · CI 五 job 全绿（GitHub API 独立确认）
> 原派：Delta（09-19 任务书）8 天零响应 → 09-27 收回重派 Alpha（`alpha-request-DK11-canvas-reassign.md`）

## 一、交付总览

| 切片 | commit | 内容 | 验证 |
|---|---|---|---|
| 切片 1 画布骨架 | `3fce7cd` | CanvasView 挂载（MainView 三态/导航/内容区分支/DesktopShell 最小挂载）+ Canvas2D 数据模型（content_ref 引用不内嵌）+ 视口变换 pan/zoom + 命中选中 + 双击新建 + 网格布局 + JSON 导出 + localStorage 独立文档 | 六断言 6/6 + A5 P95=60fps |
| 切片 2 布局与导出 | `85fc5c6` | 树形布局（edges 定父子 + visited 环防护 + 叶子堆叠父居中）+ SVG/PNG/Markdown 导出 | 十断言 10/10 |
| 切片 3 交互与性能 | `d757f1a` | Shift+拖拽连线创建（橡皮筋预览）+ LOD 三档（<0.15 色块档）+ 连线渲染 byId 索引化（O(E×N)→O(N+E)）+ aria 补全 | 十二断言 12/12 |

**合并验证**：`scripts/dk11_verify.js` 十二断言全绿（v3，Playwright 真浏览器）：

```
A1 挂载 / A2 双击新建 / A3 平移 / A4 缩放 / A5 树形坐标分布（同列唯一）
A6 SVG 产物 / A7 PNG 产物 12KB / A8 MD 产物 / A9 1000节点 P95=60fps
A10 aria / A11 Shift连线 1→2（万节点文档下）/ A12 万节点加载 191ms
```

## 二、清单五任务对照（全达成）

1. **无限画布+LOD+视口裁剪** ✅ 切片 1 裁剪/变换 + 切片 3 LOD 三档；
2. **content_ref 不内嵌正文 + 布局独立文档** ✅ 切片 1（localStorage `aurora-canvas-v1` 独立键）；
3. **三种布局** ✅ 自由（默认）/ 树形（切片 2）/ 网格（切片 1）；
4. **四导出** ✅ JSON（1）+ SVG/PNG/MD（2）；
5. **Canvas2D 起步** ✅（WebGL 按卡面定义后置 Phase 5）。

## 三、DoD 对照

| DoD | 结果 |
|---|---|
| 1000 节点 ≥30fps（桌面） | ✅ **P95=60fps**（双倍余量） |
| 10000 节点加载 <2s | ✅ **191ms**（十倍余量——byId 索引化 + LOD 色块档为决定因素） |
| ≥25fps（移动） | ⏸️ **挂起**：移动端画布不在本卡范围（desktop 领地卡），随移动画布立项兑现 |
| CRDT 协同编辑 | ⏸️ **挂起**：需 Rust 侧 loro 支撑，走 `alpha-request` 提案后实施；与 Phase 5 WebGL 同期评估 |

## 四、诚实化挂起与欠账

1. **CRDT 协同**：未实施。前置提案：aurora-core 引 loro（上游 issue 另有未决项）——建议独立切片，勿混入 UI 迭代；
2. **移动端画布**：本卡零覆盖（DK-11 卡面即 desktop 域）；移动 DoD 指标挂起；
3. **布局数据 Rust 侧存储**：现 localStorage 本地持久化，「Rust 侧存储走提案」为切片 1 时既定边界——画布文档入库（多设备同步前提）与 CRDT 同批提案；
4. **PNG 导出尺寸上限**：16384px 硬防护（超出静默跳过）——超大画布分块导出未做，挂起。

## 五、过程记录（方法论沉淀）

- **A3/A5 断言教训**：断言前先想清被测对象正确行为（拖撞节点合法 / 父子 y 对齐合法）——两次 FAIL 都是断言设计错而非实现错；
- **跨行类型联合 patch**：python 锚点用完整块匹配，单行正则会漏（dragRef 声明 5 行跨行）；
- **性能定位**：`nodes.find` 这类隐性 O(E×N) 比显式 LOD 更值得先修——191ms 的主贡献是索引化而非渲染降级；
- **原 Delta 收回重派模式**验证：Alpha 亲自交付三切片（3fce7cd→85fc5c6→d757f1a）共 3 会话（约 5.5h 有效工时），对比原卡 45 人日估算的偏差来源（任务书写的是「含内核全量口径」）。

## 六、验证链

- 本地：`tsc --noEmit` + `vite build` + `dk11_verify.js` 12/12（每次 patch 后全量回归，aria 尾补丁亦复跑）；
- CI：`3fce7cd` / `85fc5c6` / `d757f1a` 三 commit 五 job 全绿（Clippy/Rustfmt/desktop-check/MSRV 1.91/Test stable），GitHub API 独立取数；
- 交互冒烟：创建→连线→拖动→刷新布局保持（任务书验收 demo，A2/A3/A11 覆盖）。

— Alpha 2026-09-28 · DK-11 主体闭环，CRDT/移动端挂起项移交后续卡
