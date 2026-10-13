# DK-14e.1 Alpha 验收注记（关账）

**审核**: Alpha 2026-10-13 15:18-16:10 ｜ **交付**: Bravo ｜ **commit**: a7117c8（feat）+3583979（fix）直推 main ｜ **结论**: ✅ 关账，1 项 Alpha 代修（stdin/stdout 收紧）

## 终证

- **CI 八门绿**（3583979）：六常规门+wasm 矩阵三 job（Rustfmt/Clippy/MSRV/Test/desktop/3×Wasm guest build）
- **本地全绿**：cargo test 95 单测+**e2e 真组件全链 ok**（真 wasip2 组件被 wasmtime 36.0.17 实例化+init/handle-event 导出调用成功）+clippy 干净+fmt 1.91.0 过
- 收紧后回归：95 单测+e2e 5.90s ok+clippy+fmt 全绿

## DoD 核对

| DoD | 结论 | 依据 |
|---|---|---|
| 三插件单一事实源编译过+逻辑单测 | ✅ | hello（分支响应升级）/wordcount（count 纯函数+`[wordcount] {json}` 单行输出）/auditlog（被动监听 `[audit]` 行）；cdylib/rlib 双 target 逻辑宿主侧可测 |
| CI 矩阵三 artifact | ✅ | matrix.guest 三 job 绿；`${{ matrix.guest }}-wasm`；五命令 exclude 清单同步扩展 |
| 宿主 API 面零改动 | ✅ | invoke/register/install_component 签名零变更；WASI 挂载=WasmStoreCtx 内部结构+wasmtime-wasi 36 同窗（技术必需：真 wasip2 组件必带 wasi:cli/stderr 等 import——wat fixture 无 import 面不触发；aurora-plugin.wit 合约未扩，export-only 维持） |
| 日志前缀约定文档 | ✅ | docs/plugin-log-conventions.md（stderr 单行约定，测试可断言） |
| e2e 真组件集成 | ✅（超工单落地） | install→register→invoke init+handle-event 三插件全链；现场构建+缺失 skip（rustup toml 自动装 target，CI/本地均实证真跑） |
| CI 全门绿 | ✅ | 八门 |

## 设计亮点

- **导出寻址双路径**（正确版）：`get_export(None, "aurora:plugin/host@0.1.0")` 拿 instance index → `get_export(Some(&iface_idx), flat_name)` 嵌套查平名 → `get_func` → `typed()`——与 wasmtime 36 `InstanceExportLookup` 语义精确对齐（`ComponentExportIndex::lookup` 仅原样返回自身 index——**wasmtime 源码取证**）；wat fixture 平铺回退保持 d2 测试零破坏
- 寻址 fix 过程透明：3583979 message 披露 `get_typed_func` 直传 instance index 的坑+三迭代实证链

## Alpha 代修（1 项，披露）

- **inherit_stdin/stdout 收紧为仅 inherit_stderr**（wasm_runtime.rs:492-498）：原交付三件全继承——插件=不可信代码，读宿主 stdin/写宿主 stdout 超出 14e 日志约定（stderr 单信道）必要面，违最小权限红线。一行级 diff+注释披露来源；回归全绿。用户可见 stdout 输出属后续卡议题

## 审核过程注记（流水线教训）

1. **wip 分支迷雾**：Bravo 同时存在 main 直推（a7117c8+3583979=真交付）与 `bravo/dk14e1-wip`（5b09474=**旧重写版**：寻址未修复+fmt 偏差+**误删两份 Alpha 过程文档**）。Alpha 初审误落分支版，e2e 首败+fmt 失败均为分支版假象。**遗留分支建议删除**（无独有价值内容）；下张工单重申：单交付线直推 main，wip 分支即删
2. e2e 本地首败排查链：组件解剖（wasm-tools component wit——导出面正确）→wasmtime 36 源码取证（lookup 语义）→版本对证（工作区 HEAD≠分支预期）——**审核先对齐版本再判代码**
3. 沙箱重启致 nohup 编译截断+504×3：长编译任务一律 `(nohup ... &)` 圆括号模式+分段 poll

## 流水线状态

**DK-14 全卡收官**：14a 伪成功清除→14b Epoch→14d 合约/真执行/guest 构建链→14e.1 示范插件三模式+e2e 权威证明。插件生态从零到"第三方可照模板写插件"完整闭环。**待拍板**：14c Capability 资源化（capability 串→真 token）是否启动；T1-T8 台账未变（T8 与 RV-01 合流待对账）。
