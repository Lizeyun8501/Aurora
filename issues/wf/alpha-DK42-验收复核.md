# Alpha · DK-42 验收复核 — PASS（折中方案落地，裁决项 3 关账）

**复核对象**: ae182cf（Bravo 07:34 直推 main）｜**复核**: Alpha 2026-10-08 08:15
**依据**: alpha-SandboxLimits移植评估.md（295a323）——折中方案（软拒绝+触顶置位）被采纳实施

---

## 一、结论：**PASS——SandboxLimits 移植闭环，插件市场前置就绪**

| 门 | 结果 | 说明 |
|---|---|---|
| diff 审 | ✓ | SandboxLimits（Copy/serde/Default 2s+64MiB）+ PluginManifest.sandbox `#[serde(default)]` 旧清单零迁移；invoke per-plugin 覆盖+0 值回退 runtime 缺省；per-call `Store<MemoryLimiter>` 新建（hit_limit 天然复位）；两路 call 后 record_hit_limit；**软拒绝语义未变**（`Ok(false)` 保持，grow 返 -1）——折中方案忠实落地 |
| DoD 复跑 | ✓ **本地三测+全量 442 绿** | dk42_manifest_hit_limit_observable（256KB 覆盖→触顶→旗标断言+正常调用复位）/dk42_sandbox_serde_default_compat/dk42_error_classification_deadline + DK-33 回归 2/2 |
| 可观测性 | ✓ | `last_call_hit_limit(plugin_id)` 宿主程序化查询 + warn 日志 + Err 路径 classify（Interrupt trap→plugin deadline exceeded——超时/bug 可区分）|
| 四门 | ✓ | clippy 本地零警告 + **CI badge passing**（ae182cf）|
| 领地 | ✓ | wasm.rs/plugin_runtime.rs/plugin lifecycle 构造点 1 行——零越界 |

## 二、设计确认（与评估文档对照）

- **软拒绝保持** ✓（评估折中方案的兼容性前提成立——无硬拒绝的行为破坏面）
- **触顶置位** ✓（`memory_growing` 返 `Ok(false)` 同时置位——软语义+可观测兼得，无需硬拒绝的状态位补偿设计）
- **分类自洽** ✓：软拒绝下内存超限不产生 trap（grow 返 -1），memory 观测走 `hit_limits` map 而非 Err 分类——Err 路径仅 deadline/通用两类，与折中设计推演一致（grow 循环→软拒绝死循环→epoch 打断→classify deadline+hit_limit=true 双线索）
- Bravo 本次**直接在 main 上工作**（无 dev 分叉）——硬前置违规三连后的工作流修正实证

## 三、裁决清单状态

1. ~~38a/b/c 缓派~~ 关账（裁决 A）
2. ~~40b 收口~~ 关账（并入 DK-41+远期）
3. ~~SandboxLimits 移植~~ **关账（本卡 PASS）**
4. 分支清理执行（两 wf 老分支可删；dev 实体已全部落后于 main）
5. Bravo 工作流沟通（三度未 pull 后今晨已改直推 main——待 TA 确认此为最终流程）

## 四、流水线状态

V1 同步主线闭环 + 插件沙箱可配就绪——**无在途卡**。下一迭代候选（供 TA）：
- 插件市场迭代（DK-14 面启用 manifest 级限额）
- relay 会合（远期）/ 真实双机实测（产品化）
- notesnap 多用户 / bench 落表（需 token，挂账中）
