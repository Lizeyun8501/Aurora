# Alpha · SandboxLimits 移植评估 — TA 裁决项 3 决策材料

**撰写**: Alpha 2026-10-07 22:37｜**依据**: Bravo DK-39 重做（8cbc4ab）vs main Alpha 版（914d7a8）全量对比
**性质**: 评估结论，供 TA 裁决——不自行开工

---

## 一、两版 DK-39 实现对比（独立实现，无继承关系）

| 面 | main Alpha 版（914d7a8，已验收） | Bravo 重做版（8cbc4ab，未带入） |
|---|---|---|
| epoch deadline | per-call `set_epoch_deadline`（缺省 2s）+10ms ticker | 同（缺省 2s）|
| 内存限额 | `MemoryLimiter` **软拒绝**（`Ok(false)`→guest `memory.grow` 返 -1 优雅降级，wasmtime 惯例）| **硬拒绝**（`Err`→trap）+ `LimiterState` 状态位还原语义 |
| 限额可配 | **runtime 级** `with_limits(ticks, bytes)` 构造器（manifest 级留钩子未接）| **manifest 级** `PluginManifest.sandbox`（serde default 向后兼容，旧清单默认受限）|
| 错误分类 | 无——deadline trap 混在 `wasm call: {e}` 通用包装 | `classify_call_err` 三分类：memory limit / deadline / generic |
| 兼容性 | — | 旧 manifest（无 sandbox 字段）serde default 平滑 |

## 二、Bravo 独有面价值评定

1. **manifest 级可配（价值：高）**——插件市场场景刚需：不同插件差异化限额（重插件放宽/不可信插件收紧）。main 版只有 runtime 级全局值，无法 per-plugin。serde default 向后兼容设计正确（已上线清单零迁移）。
2. **错误三分类（价值：中高）**——可观测性缺陷：main 版下宿主无法程序化区分「插件超时」vs「插件 bug」vs「内存触顶」，运营排障全靠读日志字符串。分类函数小而准。
3. **状态位还原（价值：取决于语义取舍）**——Bravo 用硬拒绝故需状态位（wasmtime36 把 limiter Err 包成通用 trap 不透传 message）。**若维持 main 软拒绝语义则无此问题**（不产生 trap）。

## 三、语义取舍（TA 需拍板的唯一实质点）

- **main 软拒绝**：guest 侧 `grow` 返 -1 自行处理——wasmtime 官方 ResourceLimiter 惯例，插件可优雅降级；缺点：不可信插件可能静默行为异常（宿主不知触顶）
- **Bravo 硬拒绝**：立即 trap+显式错误——诊断友好；缺点：改变已验收的软语义（行为破坏面），且需状态位补偿 wasmtime 的 message 吞没

**Alpha 倾向折中**：**维持软拒绝 + 采 Bravo 触顶置位**（`memory_growing` 返 `Ok(false)` 时同步置 `hit_limit` 位，invoke 后宿主可查「本 call 曾触顶」）——软语义兼容保留 + 可观测性补齐，无需硬拒绝与其兼容包袱。由 TA 定夺。

## 四、结论与建议

- **建议移植（DK-42 候选卡，规模 S）**：a. `PluginManifest.sandbox` 可配（serde default）+ b. 错误三分类 + c.（视 TA 语义拍板）触顶置位
- **不移植**：硬拒绝语义（除非 TA 明确选择）
- **执行者**：Bravo 优先（状态位/分类原作者，最熟）或 Alpha 代工
- **触发时机**：插件市场迭代（DK-14 面）前完成即可，非紧急——与 DK-41（sync 主线）无依赖，可并行
- **工作量**：PluginManifest 加字段+serde default（~10 行）+ wasm.rs 接线（~40 行）+ 分类函数+测试（~60 行）

## 五、TA 裁决清单状态（更新）

1. ~~38a/b/c 缓派~~ —— **已裁决 A 关账**（DK-41 派发 7d11a91）
2. ~~40b 收口确认~~ —— **并入 DK-41+远期，关账**
3. **SandboxLimits 移植 —— 本卡材料，待拍板**（移植范围+语义取舍两问）
4. 分支清理执行（两 wf 老分支可删；dev 待 Bravo pull 后废弃）
