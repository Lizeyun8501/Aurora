# DK-15 三案验收注记 · Alpha 复核（2026-10-15 12:19 拍板执行）

**交付**: c3586d7（15a+15c）+ e3e893e（15b）｜ **复核**: Alpha 2026-10-15 ｜ **结论**: **15a/15b/15c 有条件关账**——语义复核全过，e2e 本地复跑挂起（见三）

## 一、独立复核证据（非 commit msg 转抄）

### 代码语义抽查（逐项实证）

| 项 | 抽查点 | 结果 |
|---|---|---|
| 15a fail-closed | `lifecycle.rs:176` 拒绝路径：`"unknown permission \`(p)\`; valid set: [fs, network, native, storage, clipboard, notifications, dialog]"`——诊断含非法串+合法清单 ✓；`Plugin::new` Result 化+load 透传 ✓ | ✅ |
| 15a TS 变体清理 | 旧变体（file_system/database/editor/ai/sync/system_settings）残留 grep：3 文件命中全部甄别为**同名不同义**（events.ts `'sync'`=SyncEvent kind、settings.ts `'ai'`=设置域、editor-lab=静态实验页）；capability 消费方仅 index.ts re-export+MarketplaceView——**零破坏成立** ✓ | ✅ |
| 15b 审计 | `print_first_called` 差异化审计字段在案（首调/越权模式与 log/read/query 一致）✓ | ✅ |
| 15b 缓冲 | `USER_OUTPUT_CAP = 64*1024` 常量+`drain()` 拉取即消费 ✓ | ✅ |
| 15c diff 边界 | `git show --stat c3586d7`：apps/desktop/src 三文件+shared/types+aurora-plugin——**src-tauri 零改动 ✓**；capBadge 48 行纯函数+MarketplaceView 74 行 | ✅ |
| 单元测试 | `cargo test -p aurora-plugin`：**95 passed / 0 failed** ✓ | ✅ |

### e2e 复跑波折（如实记录）

初跑 e2e 2 例挂（useroutput_denied/useroutput_pull）→溯源=**本地 wasm 产物陈旧**（我 build 用错包名 aurora-plugin-hello-guest→正确名 plugin-hello-guest→再挂=wasm32-wasip1 target 本环境未装）。**14c2 SOP 教训两次应验**（"pull 后先 build wasm 再 test"——写进 SOP 的东西必须先做）。target 安装中遇沙箱通道不稳，e2e 复跑挂起（见三）。

## 二、有条件关账判定依据

1. **代码语义层全过**：拒绝路径/审计/缓冲/diff 边界/消费方零破坏逐项实证
2. **单元 95/95 绿**（本地真跑）
3. e2e 13/13 绿为 **Bravo 声称+CI 绿**——初跑 2 挂已溯源为本地环境假挂模式（旧 wasm 产物），非代码问题：挂例恰为新 e2e（旧产物无 print/新 manifest），单元与旧 e2e 全绿
4. 挂起条件见三——**两条任一满足即无条件关账**

## 三、挂起项（结案条件）

- **H-15-1**: e2e 本地复跑 13/13 绿（wasm32-wasip1 target 装完后 `cargo build -p plugin-hello-guest --target wasm32-wasip1 --release && cargo test -p aurora-plugin`）
- **H-15-2**: 或 CI 主分支 e2e 工作流绿（下轮例行核对即可）

## 四、15a 缺口注记（不阻塞关账，入台账）

"未知 capability 串拒绝加载"的**直接单测**未独立成例（lifecycle.rs:779 为 unknown *plugin* 测试；176 拒绝路径由 manager 层测试间接覆盖）——补一例 `unknown_permission_rejected` 含 position 断言，**搭车 DK-16 L1 卡**（届时 permission 解析面必然重触碰）。台账号：C-15a-test。

## 五、后续推进

DK-15 三案收官 → **DK-16 L1 权限模型**立项（C-ns namespace+per-method 细粒度+15a/15b 铺垫收口）：设计稿 `DK-16L1-设计稿.md` 随附。
