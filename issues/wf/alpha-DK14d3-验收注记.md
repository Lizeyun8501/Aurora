# DK-14d.3 Alpha 验收注记（关账）

**验收**: Alpha 2026-10-13 08:33 ｜ **结论**: ✅ 关账，零偏差 ｜ **commit**: 0ccc3a6

## 终证

- **CI 六门绿** + 新门 **Wasm guest build (DK-14d.3)** 首证绿（Perf baseline 周期门今日正常缺席）
- Test/Clippy/MSRV/Rustfmt/desktop-check 保持绿；宿主面零改动（diffstat 仅 ci.yml/Cargo.toml/toolchain/guest crate/Cargo.lock）

## DoD 核对

| DoD | 结论 | 依据 |
|---|---|---|
| wit-bindgen 从 wit/ 生成+编译过 | ✅ | `wit-bindgen = "0.57.1"` workspace 统一（Cargo.lock 传递链已有，零新增编译面）；`generate!({path: "../aurora-plugin/wit", world: "plugin"})` 单一事实源；CI 绿 |
| CI wasm-guest job 绿+artifact | ✅ | build→upload-artifact v4（retention 30 天，`if-no-files-found: error`，产物不入库） |
| 类型校验闭环 | ✅ | wasm-tools **v1.261.0 钉死**（CLI 1.26x 线与 crate 0.24x 线不同——Bravo 自纠误判）`component wit` 反推二进制 WIT→grep 三断言（export aurora:plugin/host@0.1.0 / init: func / handle-event: func） |
| 宿主零改动零回归 | ✅ | diffstat 无宿主源文件；六门绿实证 |
| CI 全门绿 | ✅ | 六门+新门 |

## 设计加分

- **workspace 编译集隔离**：members 加 guest 但 `default-members` 不加，CI clippy/test/build 三命令 `--exclude plugin-hello-guest`——host 编译集不含 guest，wasm job 专编。干净。
- **toolchain 双保险**：rust-toolchain.toml `targets=["wasm32-wasip2"]`（工单首选方式）+dtolnay action 显式透传 targets——CI/本机一致不漂移。
- **版本判据披露**：wit-bindgen 0.57 Guest trait 生成结构实证（`exports::aurora::plugin::host::Guest`，方法无 self——三迭代试错披露）；wasm-tools CLI/crate 版本线区分教训入 workflow 注释。

## 流水线状态

**DK-14 基建全链闭环**：合约（d1）→ host 绑定+真执行（d2）→ 真 guest 构建+类型闭环（d3）。插件系统从零到"能编译真 guest 并被宿主调用链完整覆盖"。剩：**14e 示范插件**（纯业务活，guest 侧加真功能+可能的 demo 注册流程）收官；**14c Capability 资源化**（capability 串→真 token）。

**DK-14 技术债台账**：T1-T8 状态未变（T8 bench 与 RV-01 部分合流待正式对账）。
