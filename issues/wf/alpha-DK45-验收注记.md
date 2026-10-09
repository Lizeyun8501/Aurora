# DK-45 Alpha 验收注记（关账）

**验收**: Alpha 2026-10-09 08:30 ｜**结论**: ✅ 通过关账 ｜**commit**: a2ac853

## 终证渠道

- **CI 五检查全绿**（a2ac853）：Test(stable) / MSRV(1.91) / desktop-check / Clippy / Rustfmt
- 本机环境战报：磁盘 100% 满（target 20G 累积）→ cargo clean 回收 21G；GTK3 dev 库随环境重置丢失（sudo 装不回）→ tauri 面**以 CI desktop-check 为终证**（本就是 DESK 门第一渠道）；本地核心面分段增量编译推进至 wasmtime 段

## DoD 核对（代码级精读）

| DoD | 结论 | 依据 |
|---|---|---|
| 1. command 闭环 | ✅ | build_policy_list 纯函数 + e2e set→list roundtrip（publisher 隔离/configured 标记/缺省展示断言齐） |
| 2. UI 编辑生效 | ✅ 语义达成 | 编辑即写 invoke('cmd_set_plugin_policy')，wifi_only 同语义 |
| 3. 安装路径锚定 | ✅ | e2e 双场景：已配置→manifest.sandbox==limits；未配置→None 不动（DK-14 接入即用） |

## 质量亮点

- 纯函数+command 薄壳分层（单测直接覆盖纯函数）
- DTO `configured` 标记区分显式配置与缺省展示
- `0` 值语义注记正确引用 DK-42（invoke 侧回退 runtime 缺省）
- 原语只读复用零改动；领地干净（禁触四张刚验收面零触碰）
- 测试落 bootstrap 可链接面（desktop lib 不链 webkit 的语义等价策略——webkit 环境依赖的可持续规避范式）

## 验收注记（两处，不阻塞）

1. **UI 形态偏离**：交付为命令面板两项（查看列表+prompt 三连编辑），非开工令的 settings 页分组（WifiOnlyRow）。语义（编辑即写+列表查看）达成；settings 页形态可并入 relay 后微卡或 DK-14 时统一。命令面板为 DK-44 后 desktop 现有快速通道，接受。
2. **publishers 名单来源**：cmd_list_plugin_policies 的 publishers 由前端传入（命令面板演示态）。**DK-14 市场安装接入时须改为宿主侧枚举已装插件**（BootedApp 有插件注册表后由后端出名单）——已写入 DK-14 前置注记。

## 流水线推进

DK-45 关账 → **relay 会合卡（四案排序第 3 案）派发**，预探材料 17418e7 就绪。
