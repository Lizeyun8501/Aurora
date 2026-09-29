# Alpha DK-10 切片 6 复核回执（自领自复核）— MCP 网关鉴权分级

- **对象**：`8ea9a18`+`97fc2b3`（lock） · **日期**：2026-09-29 15:20–16:00
- **结论**：**本地矩阵通过（ai 183 passed + clippy 0 + fmt workspace 净）；CI 终态
  下会话首查（连同 13d5ace 已确认全绿）**

## 一、交付面（卡面「鉴权分级：stdio/回环默认信任，仅对外 HTTP 走 OAuth/HMAC」落码）

`crates/aurora-ai/src/mcp_auth.rs` 新建（416 行）：

1. **三级信任裁决（落码即文档）**：
   - `Stdio`（本地子进程管道，同 OS 信任域）→ 默认信任；
   - `LoopbackHttp`（127.0.0.1/::1，进程边界=用户边界）→ 默认信任；
   - `ExternalHttp` → **必须凭证，无凭证 fail-closed 拒绝**（MCP 工具可触发 AI 链，
     绝不能裸暴露——MissingCredential 拒绝码）；
2. **HMAC-SHA256 请求验签**：签名材料 `key_id | timestamp | method | path |
   SHA256(body)`（含 body 哈希防篡改）；**防重放 5min 窗口**（时钟注入可测——
   `with_clock`）；key 注册表查表（未知 key → UnknownKey 拒绝，不可枚举探测）；
3. **OAuth Bearer 结构面**：token 前缀+非空校验（真实 IdP introspection 挂后续片）；
4. `McpServer`（既有协议派发器）之上独立成 gate 层——鉴权先于 dispatch（装配纪律）。

测试 6/6：信任分级三态 / 对外无凭证拒 / HMAC roundtrip+篡改 body 拒 / 重放窗拒 /
OAuth 结构 / 未知 key 拒。hmac 依赖入 Cargo.toml（workspace 0.12）。

## 二、事故与修复（重要教训）

1. **python 注册脚本切片漏尾 → lib.rs/Cargo.toml 截断**：`s[:m.end()] + 新行` 少了
   `+ s[m.end():]`——模块注册两次踩同坑（205 行 lib.rs 只剩 22 行）——**git checkout
   恢复 + 行锚点重写法**（`s[:line_end+1] + new_line + s[line_end+1:]`）——**凡
   "在 X 行后插入"必须含尾巴拼接，或改用行锚点插入法**；
2. **desktop-check 连续三轮❌**（put→set 修后仍有 Vec/&[u8] 引用缺失）——本地无 GTK
   验不了 desktop 的盲区，靠 CI 注解逐轮收口——13d5ace 全绿确认。**教训：写 tauri
   命令时 KVStore::set(key, &[u8]) 参数一律 `&vec` 显式引用，别传值**；
3. 测试自引用残废预防：写测试时用到的方法先 rg 确认存在（本轮 matches! 改写一处
   虚构 code_only API）。

## 三、CI 终态汇总

| commit | 五 job | 备注 |
|---|---|---|
| 9fd130d/58e1ea0/3fc32e8 | desktop ❌ 余四 ✅ | Vec→&[u8] 缺 & |
| 13d5ace | **全绿** | set 引用修正——切片 4/5 tauri 面 CI 闭环 |
| 8ea9a18/97fc2b3 | 下会话首查 | 切片 6 MCP 鉴权（本地 183 绿） |

## 四、DK-10 大卡进度（55 人日）

- ✅ 切片 1 策略接线 / 2/3 云门禁 / **4 两段式提交** / **5 Agent 会话面** /
  **6 MCP 鉴权分级**（本片）
- ⏸️ 时间轴可视化（含前端 Agent 面板+提案审查对话框——UI 大面收尾片）/
  Shared-Public 路由分级（待多工作区）→ **DK-10 core/ai 面基本收官，剩 UI 收尾**

— Alpha 2026-09-29 傍晚
