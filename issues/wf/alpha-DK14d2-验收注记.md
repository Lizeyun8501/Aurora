# DK-14d.2 Alpha 验收注记（关账）

**验收**: Alpha 2026-10-12 18:58 ｜ **结论**: ✅ 关账，零偏差 ｜ **commit**: 0d25d25 ｜ **生效**: CI 出现第六门 Perf baseline（RV-01 周期卡 41ac132，周一 03:00 UTC+dispatch+`if: always()`，绿）

## 终证

- **CI 六门全绿**（0d25d25）：Test / MSRV 1.91 / desktop-check / Clippy / Rustfmt / **Perf baseline（新门首证）**
- Test 含新增六测：invoke init 全链 / handle-event 全链 / 实例缓存命中 / unregister Drop / **epoch 杀死 spin guest** / 无 component 错误路径

## DoD 核对

| DoD | 结论 | 依据 |
|---|---|---|
| 假确认 JSON 段删除 | ✅ | -23 行含假 JSON+全部"模拟执行"注释；`rg "模拟执行"` 代码面待复核零残留（diff 已见删净） |
| 全链真执行 | ✅ | `load_component → Store::new(WasmStoreCtx) → set_epoch_deadline → Linker → instantiate → get_typed_func 真调用`（L448 注释自述+两全链测试实证） |
| method→WIT 映射 | ✅ | `"init" =>`（单参 capability）/`"handle-event" =>`（三参 capability/event/payload）——**严格按 d1 合约签名**（`(&str,), (Result<(),()>,)` / `(&str,&str,&str),`）；其余→InvalidInput 不再假 ok |
| capability 串派生 | ✅ | `derive_capability_string`（注册 Manifest granted 集合逗号连接）——14c 资源化时换真 token 的占位点清晰 |
| epoch 对 guest 生效 | ✅ | `test_epoch_deadline_kills_spin_guest`（guest_spin.wat 死循环 fixture 被打断→错误返回）；ticks 可配置（`set_epoch_deadline_ticks` pub API） |
| 实例生命周期 | ✅ | 缓存命中测试+unregister Drop 测试；per-plugin 锁粒度（同插件串行 Store &mut 不可重入、跨插件并行）——借用纪律注释到位 |

设计加分：`PluginInstance` 用 `Arc<parking_lot::Mutex<>>` 包裹，缓存读锁短持、调用期仅持插件级锁——并发形状正确。

## 流水线状态

**插件执行链真通**：宿主→guest 导出调用（init/handle-event）+宿主函数真实通道+epoch 抢占三面俱全。aurora-plugin 从"骨架+模拟"变为"骨架+真执行（单方向）"。剩：**d3**（guest 构建链 wit-bindgen+wasm32-wasip2+CI wasm job）→ **14e** 示范插件；14c Capability 资源化。

**旁路发现**（RV-01 41ac132 首跑）：index-rebuild 1k=20.4s（20ms/条——`index_note` 每 call 新建 writer 50MB 开销主导；10k 并发超线性未完）——DK-03 优化实证输入，潜在性能卡素材，已入 bench.yml 周期门可持续观测。
