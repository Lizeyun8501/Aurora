# DK-14d WIT 高风险单独勘察报告

**勘察**: Alpha 2026-10-12 08:35（等待 DK-51 交付窗口）｜ **结论**: 高风险定性**证实且加重**——WIT 是第 3 层，其下 L1 加载/L2 执行全空壳，真实组件化=从 L0 到 L3 全链路新建 ｜ **commit**: 待推

## 一、分层真相（比预备稿"WIT 高风险"更深一级）

| 层 | 现状 | 实证 |
|---|---|---|
| L0 引擎 | ⚠️ 浅挂 | `wasmtime = "36"`（锁 36.0.17）workspace 统一；但 `WasmRuntime` 仅持有 `Engine::default()`（wasm_runtime.rs L262）——**无 Config**：epoch 计时/ResourceLimiter/`wasm_component_model(true)` 均未配置 |
| L1 加载 | ❌ 零 | 全 crate 无 `Module::new`/`Component::new` 加载路径 |
| L2 执行 | ❌ **伪执行** | wasm_runtime.rs L331-332 注释自白："若方法名匹配某宿主函数，则按宿主函数分派（**模拟**插件回调宿主）；否则**模拟执行**插件导出函数并返回确认结果" |
| L3 WIT | ❌ 零 | `wit-bindgen`/`wasm-tools`/`component` 全库零命中 |

**定性修正**：aurora-plugin 3618 行是"接口骨架+模拟实现"。预备稿"wasmtime 已挂"应修正如实表述为"**wasmtime 已进依赖树，运行时未挂**"。

## 二、版本矩阵（外部风险大部分可排除）

| 组件 | 结论 | 依据 |
|---|---|---|
| wasmtime 36.0.17 | ✅ Component Model 稳定区（24+ 稳定，36 成熟） | 版本线 |
| MSRV 兼容 1.91.0 | ✅ **已被现有 CI 实证** | wasmtime 36.0.17 已在 Cargo.lock（9406）且 CI 五门（含 MSRV 1.91 门）全绿——无需外部核实，本仓库已证明可编译 |
| host 侧绑定 | ✅ **零新增依赖** | `wasmtime::component::bindgen!` 宏内置——host 不需要 wit-bindgen |
| guest 侧绑定 | ⚠️ 需引入 wit-bindgen | 版本须与 wasmtime 36.x 配套（开工时以 wasmtime 36 release 配套表为准，预备稿不焊死版本号） |
| target | ⚠️ 从零 | `rust-toolchain.toml` 无 targets 字段；`wasm32-wasip2` 需 CI 侧 rustup 添加 |
| 依赖树体积 | ✅ 已被承担 | Cargo.lock 中 wasm 相关 crate 37 个（wasmtime* 16 + cranelift* + wasmparser 等）已在编译——component 路径是 feature flag 不是新 crate 树 |

## 三、风险清单（14d 开工令素材）

1. **Engine Config 重构波及构造 API**：`Engine::default()` → Config（component_model + epoch + ResourceLimiter）——**14b Epoch 应并入 14d 首段**（一次 Config 重构挂两个 feature，预备稿"14b 独立可并行"**修正为"14b 并入 14d"**）。DK-39 的 ResourceLimiter 签名错误类型经验（workspace 显式声明传递依赖）直接适用
2. **权限桥接前置设计**（14c/14d 耦合）：permission.rs 已有 `Permission::as_capability()`（L52）映射——WIT host 函数分派须按 capability 收口，合约设计要前置到 14d 首段，否则 L2 做完返工
3. **guest 构建链从零**：CI 无 wasm job、无 wasm-tools CLI、rust-toolchain 无 target——14d 须带 CI 扩展（新 job：guest 构建+bindgen 校验）
4. **iframe_runtime 同属模拟态**（范围外披露）：iframe_runtime.rs L223"模拟 postMessage 收件箱"、L356"同步请求-响应往返…模拟框架处理"——前端桥接层，14d 不碰，但**伪成功清单应补录**（与 hot_update 同类）

## 四、与 14a 的关系裁决

伪成功两处实锤定位：hot_update.rs L220-226（"模拟后台下载 I/O"+sleep 1ms 假下载→Pending）与 L240-246（confirm_switch 内 should_fail 测试钩子）。

- **14a 范围不变**：清"假报成功"——hot_update 模拟下载改真实现或显式 `NotSupported` 错误返回（不静默成功）
- **14a 不碰 wasm_runtime**："模拟执行返回确认"（L331）属 L2 未实现而非假报——未实现走 feature gate+文档披露，由 14d 真实现替换。两类"伪"分流，避免 14a 越界到执行链

## 五、拆卡建议

**14d 维持单卡，内部三段独立验收**（预计 6-8 人日，较预备稿上调）：

| 段 | 范围 | DoD 核心 |
|---|---|---|
| d1 | L0+L1：Config 重构（component_model+epoch 并 14b）+ Component 加载路径 + 最小 WIT 合约（host bindgen!） | 真 wasm 载荷 Component::new 成功；WIT 合约含 capability 参数位 |
| d2 | L2：真执行链替换 L331 模拟段 | 导出函数真实调用+宿主函数真实回调；模拟段删除 |
| d3 | guest 构建链：wit-bindgen + wasm32-wasip2 + CI job | CI 有 wasm 构建门；示范 guest 编译过 |

**建议执行序**：14a（XS-M，先清假报）→ 14d.d1（Config+加载）→ d2/d3。14c Capability 设计前置在 d1 的 WIT 合约里消化。

## 六、对预备稿的三处修正

1. "wasmtime 已挂" → "wasmtime 已进依赖树，运行时未挂（Engine 孤挂无 Config）"
2. "14b 可并行" → "14b 并入 14d.d1（Config 一次重构）"
3. "14d 高风险" → "证实；且为全链路新建（L1-L3 零基础），估时上调至 6-8 人日"
