# DK-15 结案+Alpha 独立审计注记（2026-10-15 13:53 等待期任务）

**性质**: Alpha 独立推进（等待 Bravo DK-16 交付期）——H-15-1 清账+15a/15b 新面深度安全审计 ｜ **结论**: **DK-15 无条件关账**；审计 3 注记 0 阻塞

## 一、H-15-1 结案：e2e 本地 13/13 全绿

`cargo build -p plugin-hello-guest --target wasm32-wasip2 --release && cargo test -p aurora-plugin`：
- 单元 **95/95** + e2e **13/13** 全绿（wasip2, release）
- DK-15 三案由"有条件关账"升 **无条件关账**（H-15-1 满足，H-15-2 不再需要）

### 假挂归因最终修正（SOP 修正）

14c2 注记"SOP——pull 后先 build wasm 再 test"中的 target 写为 **wasm32-wasip1——笔误**；CI 真源（ci.yml:48/51）= **wasm32-wasip2**（rust-toolchain.toml 已声明 targets）。初跑 2 例挂的完整归因链：错误包名（aurora-plugin-hello-guest→真名 plugin-hello-guest）掩盖了错误 target（wasip1 本环境无产物）→"旧产物"定性不完整，实为**从未构建成功**。**SOP 修正版**：`cargo build -p plugin-hello-guest --target wasm32-wasip2 --release` 后再 test；target 以 ci.yml 为准，不信注记。

## 二、Alpha 深度审计：15a/15b 新面

### 锁序审计（无死锁面）✅

三层锁拓扑：`instances`(per-plugin parking_lot::Mutex) → `user_outputs` 外层 RwLock → slot 内层 RwLock。
- print 路径：instance 锁 → slot.write（不碰外层）
- pull_output 路径：外层 read → slot.write.drain（不碰 instance）
- 实例化：外层 write（短域，scoped 653-661 行，Arc 注入后即释）
- **无任何路径持内层后取外层**——锁序单向，无死锁面

### 审计发现（注记级，不阻塞）

**A-1（pull_output 无鉴权——by design，L2 注记）**：`call_host_function(PullOutput)` 直通（851-860 行）。当前可达性安全：调用链=宿主进程内 `runtime.invoke("pull_output")`，**guest 无自助拉取面**（WIT 无此导出）。但若未来 invoke 暴露到 IPC/FFI（mobile-ffi/desktop ipc），需调用方鉴权（桌面 session 校验）——**挂 L2 候选清单**。

**A-2（失败分级原则确认——建议入档推广）**：print 越权=trap（CapabilityDenied 硬拒绝，防探测重试）；缓冲单条超限=内层 Err（软失败，955 行 push false 分支）。与 14c.4 教训"guard 内层 Err 非 trap"不矛盾——**分级原则**：能力面违规=trap（安全边界），资源面超限=内层 Err（正常失败路径）。建议作为 14c.4 教训的推广条目入档，DK-16 L1 前缀拒绝语义对照（denied=trap 面，与越权一致 ✓）。

**A-3（UI 级 capability 占位语义——维持既有定性）**：两层识别（Capability 含 clipboard/dialog/notifications；Permission=沙箱子集）——UI 级变体合法但无沙箱映射，token granted 含占位变体无运行时语义。14c.3 已定性，15a 类型化未改变——**维持**，DK-16 L1 不触碰。

### 缓冲边界正确性 ✅

单条 l+1>CAP 前置拒绝（整条拒，无 UTF-8 截断面）；FIFO 淘汰循环有界（前置检查保证可容纳）；bytes 记账 saturating_sub 对称；drain 清零一致。

## 三、状态

- DK-15：**关账**（三案全绿）
- DK-16：Bravo 交付中（本档不触碰其工作面）
- 台账：C-15a-test 仍挂（DK-16 搭车）；新增 L2 候选=A-1
