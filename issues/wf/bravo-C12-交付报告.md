# C-12 交付报告：recentCommands 上移 core KV——双端最近使用单源

> 执行: Bravo ｜ 工单: TA 2026-11-20 07:19 ｜ 依赖 B-02（面板结构稳定）

## 交付
### ① core 域（crates/aurora-core/src/palette_recents.rs——task_store 同款惯例）
- KV 单键 `palette:recent_commands`（工单字面）;值 `[{id,ts,count}]` serde 蛇形
- `get_recent_commands(kv)`:读+解析;坏 JSON 自愈→空表（UI 偏好域口径——损坏重置,track 覆写即恢复;与 task_store fail-closed 差异已注记缘由）
- `track_recent_command(kv,id,ts)`:LRU 合并（同 id→count+1+ts 刷新+移队首;新→count=1 头插）+上限 50 截尾;id 空串 fail-closed（A04）
- 序列化防御分支 A04（理论不可达）

### ② IPC 薄封装（src-tauri/palette_commands.rs——calendar_commands 同款）
- `cmd_recent_commands_get` → JSON 串（工单字面返回形态）
- `cmd_recent_commands_track(id, ts)` → JSON 串（LRU/计数在 core——IPC 零逻辑）
- invoke_handler 注册两行

### ③ TS 镜像（shared/types/src/palette.ts——契约单一事实源）
- `RecentCommand{id,ts,count}`+`RECENT_COMMANDS_KEY`+`RECENT_COMMANDS_MAX=50` 常量
- **Rust include_str 契约测试互证**（plugin.ts capability 同款惯例——字段/键名/上限三锚比对,漂移即红）

### ④ 桌面接线（DesktopShell——铁律 6 三段式·双写段）
- **track 双写**: recordPaletteExec 保留 localStorage 写+`invoke track`（.catch 静默——工单验收「失败静默不阻断命令执行」）
- **读 KV 优先+回退**: 挂载拉 `cmd_recent_commands_get`→有值覆盖 state（KV 权威）;失败/空→localStorage 现源（工单字面「localStorage 旧源保留读回退」）
- 切读下卡=去本地兜底后 KV 唯一

### ⑤ 移动对账（对账即收）
rg 零命中:移动无命令面板（无 CommandPalette 消费面）→**仅留通道**（core 域双端共享;FFI 桥留待移动面板卡——无消费者不加空桥,总闸 4 条「明确标注+理由」）。

## 验收对照
| 条款 | 结果 |
|---|---|
| core 单测（KV 读写+LRU 淘汰+同 id 合并+TS 镜像） | ✅ **5/5**（round-trip/50 淘汰/计数合并/ts 刷新队首/坏 JSON 自愈+include_str 契约） |
| 桌面 tsc 0 | ✅（desktop+mobile 双端 RC=0） |
| 面板最近组置顶不破坏 | ✅ 回归 C-01 21/21+E2E 10/10（注入链零改动——props 形态不变） |
| 禁改 fuzzy.ts | ✅ `git diff --stat fuzzy.ts` 空输出（评分函数零 diff;加权仍在调用侧 C-06 既有形态） |
| 上报失败静默 | ✅ .catch(()=>undefined)（双处——track+get 读回退） |

## 验证矩阵
core 测试 5/5 · clippy（aurora-core+aurora-desktop --all-targets -D warnings）RC=0 · fmt RC=0 · dtsc/mtsc RC=0 · vite build ✓ · 回归双绿 · 移动对账=无面板通道留

## 总闸六条
1 CI（推送后报）2 零静默占位 ✓（失败静默=回退真源非伪数据;A04 fail-closed）3 WritePath ✓（UI 偏好域非笔记写链——注记已写明）4 双端 ✓（通道双端共享;移动消费面无=对账披露+理由）5 无障碍 ✓（零 UI 新面——纯数据层卡）6 报告五项 ✓
