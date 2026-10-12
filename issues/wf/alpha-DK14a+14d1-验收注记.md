# DK-14a + 14d.d1 Alpha 验收注记（关账）

**验收**: Alpha 2026-10-12 12:05 ｜ **结论**: ✅ 双案全关账，零偏差 ｜ **commits**: 0ac0b1a（14a）+ 647a8af（14d.d1）

## 终证

- **CI 五门全绿**（647a8af 合并态）：Test / MSRV 1.91 / desktop-check / Clippy / Rustfmt
- Test 含新增：epoch deadline 打断死循环 wasm（`set_epoch_deadline(2)`）+ Component 加载成功（component model 配置生效实证）

## 案一（14a API 卫生，0ac0b1a）DoD

| DoD | 结论 | 依据 |
|---|---|---|
| 生产 API 面无测试设施 | ✅ | `simulate_switch_failure` 移入 `#[cfg(test)]`；`switch_should_fail` 注释如实化（生产路径恒 false） |
| sleep 装饰移除 | ✅ | 1ms sleep+`Duration` import 清理；注释披露"真实网络下载属 marketplace 未来层——签名信任链已备，分发能力未建" |
| 行为零变化 | ✅ | 单文件 +15/-6；`RolledBack` 分支保留（真实状态机语义，测试驱动） |
| 调用方复证 | ✅ | 零生产调用方（勘察已实证） |

## 案二（14d.d1，647a8af）DoD

| DoD | 结论 | 依据 |
|---|---|---|
| Engine Config（component+epoch） | ✅ | `Engine::default()` 删除（-3 行=旧注释×2+构造行）；Config: component_model + epoch_interruption |
| epoch deadline 单测 | ✅ | 死循环 wasm fixture 被 `set_epoch_deadline(2)` 打断——**DK-14b 并入验证达成** |
| Component 加载路径 | ✅ | `load_component` pub API → `Component::new(&engine, wasm_bytes)`，失败路径明确错误 |
| WIT 合约 + capability 位 | ✅ | `wit/aurora-plugin.wit` 28 行：init/handle-event 双签名带 `capability: string`；**文件头含 14c 前置映射**（string→`borrow<capability-token>` 升级路径、"先校验后动作"语义保留）——超工单要求 |
| L331 模拟段零触碰 | ✅ | diff 删除行仅旧文档注释+构造行，模拟执行段原样（d2 边界守住） |
| 不引外部工具链 | ✅ | fixture 用 `wat` workspace 依赖（DK-33 惯例纯库）编译 wasm 文本，零新增构建链 |
| ticker 关停路径 | ✅ | `ticker_stop` 停机位 + `Drop` 置位后 join 不留孤儿线程——工单规约原文执行 |

## 流水线状态

**插件生态第一段打通**：wasmtime 36 从"依赖树存在"变为"真实可加载 Component + epoch 抢占"。14d 剩 d2（L331 真执行链替换）+ d3（guest 构建链+CI wasm job）+ 14e 示范插件；14c capability 资源化已有合约落位。d2 开工令待拍板。
