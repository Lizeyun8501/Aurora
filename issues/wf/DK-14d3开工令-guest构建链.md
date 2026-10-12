# DK-14d.3 开工令 · guest 构建链（wit-bindgen + wasm32-wasip2 + CI job）

**派发**: Alpha 2026-10-12 21:05 ｜ **Assignee**: Bravo ｜ **Estimate**: 3-4h ｜ **Labels**: `plugin-ecosystem`, `area/wasm`, `area/ci` ｜ **依赖**: 14d.2（0d25d25 已关账）

## 勘察基线（Alpha 2026-10-12 21:00）

- d2 已通宿主→guest 全链（wat fixture）；d1 合约 `wit/aurora-plugin.wit`（package aurora:plugin@0.1.0，world plugin export host）
- 宿主 bindgen! 用 `path: "wit"`（wasm_runtime.rs L267）——guest 侧引用同一合约文件，**单一事实源**
- rust-toolchain.toml 无 targets；CI ci.yml 三 job（fmt/clippy/test）+ bench.yml 周期门
- wit-bindgen 依赖全库零痕迹（预备稿实证维持）

## 任务

1. **wit-bindgen 引入**：workspace Cargo.toml 统一版本。**版本裁决不焊死**：以 wasmtime 36.0.17 发布配套为准（wasmtime 与 wit-bindgen 生态同步发版，实现时以 cargo add 后 guest 编译通过+宿主实例化类型校验通过为最终判据——教训⑦假设+复核）
2. **示例 guest crate**：`crates/plugin-hello-guest`（cdylib，target wasm32-wasip2）——`wit_bindgen::generate!` 引 `../aurora-plugin/wit/`（相对路径 or workspace 共享拷贝，以单一事实源为准）；导出 init/handle-event 两函数（返回 `result::ok`，init 可打 log 输出到 stderr 验证调用链）；Cargo.toml 明确 `crate-type = ["cdylib"]`
3. **rust-toolchain.toml**：`targets = ["wasm32-wasip2"]` 全局统一（体积可忽略，避免 job 间漂移）——若引发其他 job 组件安装异常则回退为 wasm job 内 `rustup target add`，报告披露选择
4. **CI wasm job**（ci.yml 增 `wasm-guest`）：`cargo build -p plugin-hello-guest --target wasm32-wasip2 --release` → 产物 `plugin_hello_guest.wasm` 上传 artifact（保留 30 天即可）；**产物不入库**（卫生——源码在库，二进制 CI 产出）
5. **类型校验闭环**（d1 合约 ↔ guest 对齐证明）：CI job 增 `wasm-tools component wit` 校验（或 cargo 组件校验步）证明 guest 导出面与 `aurora:plugin/host` 匹配——若引入 wasm-tools CLI 则版本钉死入工作流注释

**DoD**：
- [ ] guest crate 经 wit-bindgen 从 `wit/aurora-plugin.wit` 生成绑定并编译过（wasm32-wasip2）
- [ ] CI `wasm-guest` job 绿，artifact 含 .wasm 产物
- [ ] guest 导出面与合约类型校验通过（单一事实源证明）
- [ ] 宿主侧零改动零回归（本卡纯 guest 侧+CI；宿主 fixture 测试保持 wat 路径）
- [ ] CI 全门绿

## 停手边界

- **不做** 14e 示范插件业务功能（d3 只交付"能编译的真 guest"最小件）
- **不加** WIT import 面（guest→host 回调仍属 14c/后续——合约保持 export-only）
- **不动** 宿主 wasm_runtime（d2 刚关账，零触碰）
- fixture 策略不变：宿主单测仍用 wat 文本（零漂移），不改为加载 CI 产物

## 执行规约

- 独立 commit；报告含挂接清单自查节：wit-bindgen 版本与 wasmtime 36 对齐证据、target 引入方式（toolchain 文件 vs job 内 add）及理由、artifact 路径逐一列明
- wit-bindgen 版本、wasm-tools 版本写入 workflow 注释与报告（复现性）
