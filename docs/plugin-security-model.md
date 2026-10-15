# Aurora 插件安全模型总览（DK-14→17 语义基准）

**出稿**: Alpha 2026-10-15 ｜ **性质**: 架构文档——跨卡安全语义首次全局成文；DK-17 实现以此为准 ｜ **状态**: L0/L1 已落地，L2 设计中

## 一、分层模型

```
guest (wasm32-wasip2 组件, wasmtime ComponentModel)
  │  WIT aurora:plugin@0.2.0（合约版本进接口名——漏改事故教训）
  │
  ├─ L0 token 消费面（DK-14c 系列）：capability-token 资源
  │    log / read-doc / query-docs / print —— per-method require
  ├─ L1 数据隔离面（DK-16）：namespace 强制前缀 {plugin_id}:
  │    read/write/query 三环链 token→namespace→method；query 键集过滤
  └─ L2 跨插件授权面（DK-17，设计中）：share 只读授权（明门替代暗道）
       横切：pull_output（15b 用户输出，宿主特权）、CallerCtx（A-1 预留）
```

**能力栈全景**：fs/network/native/storage/log + clipboard/dialog/notifications（UI 占位，无沙箱映射，14c.3 定性维持）+ user-output（15b）。

## 二、横切不变量（违反即缺陷）

1. **fail-closed**：非法输入一律拒，不静默
   - manifest 层：未知 capability/permission/storage_method 串 → 拒绝加载（15a/16，诊断含非法串+position+合法清单）
   - id 层：plugin_id 含 `:` → 拒（16.1，A-4 堵口——namespace 前缀完整性）
   - 徽章层：TS unknown capability 变体 → fail-closed 视觉（15c）
2. **失败分级**（14c.4 教训推广，A-2 确认）：
   - **能力面违规 → trap**（CapabilityDenied：越权/namespace-denied/method-denied）——防探测重试
   - **资源面超限 → 内层 Err**（print 单条超 64KiB、缓冲淘汰）——正常失败路径
3. **审计节流**：仅首调（first-call）与越权（denied）记审计行；正常调用走 tracing 转发（内容级 debug）
4. **拉取即消费**：UserOutputBuf 64KiB FIFO，drain 清空——无推送面（T6 实时推送后可挂 event）
5. **锁序单向**：instances(per-plugin Mutex) → user_outputs(RwLock) → slot(RwLock)——无相反序（审计实证）

## 三、边界语义

| 面 | 语义 | 依据 |
|---|---|---|
| log vs print | 诊断流（tracing 转发）vs 用户可见流（缓冲+拉取） | 15b 分界文档 |
| 桌面 pull_output | 宿主进程内 invoke；**guest 无自助拉取面**（WIT 无导出） | 15b+A-1 |
| host 预置键 | 宿主直写 KV 不过 guard（特权面）；guest query 仍只见本前缀 | L1 e2e 实证 |
| UI 占位能力 | granted 含占位变体无运行时语义 | 14c.3 维持 |

## 四、接线现状（2026-10-15 审计）

- **桌面 src-tauri**：仅 `cmd_apply_plugin_policy` 沙箱策略语义（锚定注释：真实安装流程在市场迭代卡）——**未接线=无绕过面**；接线时**必须经 `Plugin::new` 强签名路径**（15a/16.1 fail-closed 已强堵）+ id 禁 `:` 校验随 Plugin::new 自动生效
- **mobile**：android 壳+web dist，无 rust 插件运行时接线（iframe 沙箱属另一体系，不在本模型范围）
- **合约版本链**：wit@0.2.0 = CONTRACT_IFACE@0.2.0 = 注释一致 ✓（15b print 已入 wit:49）

## 五、已堵面 / 待堵面台账

| # | 面 | 状态 |
|---|---|---|
| A-4 | plugin_id 前缀嵌套逃逸 | **已堵**（16.1：id 禁 `:`） |
| C-15a-test | 未知串直接单测 | **已清**（96→99 单测） |
| A-1 | pull_output 调用方鉴权 | 挂 DK-17（CallerCtx 接口位，桌面 IPC 接线时收口） |
| A-5 | self-id 派生前缀 | 挂 DK-17（init 传 plugin_id，hello 写死退役） |
| try_register | register 强签名 | 挂 DK-17（安装面接线统一收口；现全内控+堵口语义足够） |
| L2 share | 跨插件授权 | DK-17 设计稿已派（只读+opt-in+revoke） |

## 六、演进纪律

- 实现与文档冲突 → 停手报告（持续有效）
- 合约升级必查 CONTRACT_IFACE 版本串（接口名含版本）
- SOP：pull 后先 `cargo build -p plugin-hello-guest --target wasm32-wasip2 --release` 再 test（target 以 ci.yml 为准）
- fail-closed 新面：诊断必须含非法串+position+合法清单（三要素）
