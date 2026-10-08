# DK-45 交付报告：插件沙箱策略宿主入口（市场接线·四案第 2 案）

> 执行：Bravo 2026-10-08 深夜 · 依据：DK45-开工令（c363ec2）
> 前置：DK-45 前置原语 a9b039f 只读复用（core 零改动）· main 直推+desktop clippy 补丁执行
> Commit：见 git log（feat(DK-45)）

## 落地面（两块）

### 1. tauri commands（策略读写）
- `cmd_list_plugin_policies(publishers)`：per-publisher 策略列表——
  已配置项返回宿主限额（configured=true）；未配置项按缺省安全值展示
  （configured=false，serde 缺省 2s/64MiB）
- `cmd_set_plugin_policy(publisher, ticks, bytes)`：编辑即写（与 wifi_only 同语义）；
  0 值合法（DK-42 回退语义）
- `cmd_apply_plugin_policy(publisher, manifest)`：安装路径语义锚定透传——
  `apply_sandbox_policy` 覆盖/保持双场景（DK-14 市场安装接入即用）
- 纯函数 `build_policy_list` 与 command 壳分离（薄壳透传）

### 2. settings 编辑 UI（WifiOnlyRow/壳层既有范式）
- 命令面板两项：
  - 「插件沙箱策略（查看 per-publisher 限额）」——prompt 收 publisher 名单 →
    列表弹窗（配置/未配置标记 + ticks/MiB 展示）
  - 「设置插件沙箱策略（编辑即写，市场安装时生效）」——prompt 三连
    （publisher/ticks/MiB）→ set → 确认弹窗
- 编辑即写无保存按钮（与 wifi_only 即写语义一致——开工令 §2 要求）

## DoD 验证

| DoD | 断言 | 结果 |
|---|---|---|
| 1 command 闭环 | dk45_policy_set_list_roundtrip：set→list roundtrip、publisher 隔离、未配置缺省展示 | ✅ |
| 2 UI 生效 | 编辑即写 KV 持久一致（原语层保证）+ 前端 list 即时拉取（refresh 模式）+ tsc/vite build 绿 | ✅ |
| 3 安装路径锚定 | dk45_install_path_semantics_anchored：set→apply→manifest.sandbox==limits；未配置→保持缺省（DK-14 接入即用） | ✅ |
| 4 回归+四门 | workspace 全绿 + 本地五门（含 desktop clippy 补丁——开工令硬前置执行） | ✅ |

## 验证矩阵

| 门 | 结果 |
|---|---|
| TEST（全量 workspace --exclude desktop） | 0 ✅ |
| CLIPPY（workspace 全量） | 0 ✅ |
| **DESK_CLIPPY（新增补丁门）** | **0** ✅ |
| FMT | 0 ✅ |
| DESK（check） | 0 ✅ |
| dk45 | 2/2 ✅ |

## 测试位置说明（诚实化）

- DoD 1/3 语义测试落 **aurora-bootstrap/tests/plugin_policy_e2e.rs**（可链接 crate）——
  desktop lib test 本机/CI 均不链接 webkit2gtk（无系统库），tauri 层不可执行测试；
  plugin_commands.rs 为薄壳透传（无逻辑分支），语义覆盖等价于 command 闭环。

## 坑沉淀
- desktop lib test 链接需系统 GTK（fakepc 只给 .pc 元数据）——桌面面测试一律
  落可链接 crate 或 CI desktop-check（check 不链接）覆盖
- 多次 cargo 实例等锁（504 断连残留）——启动前先查 `ps aux | grep cargo` 清残留
