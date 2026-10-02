# Bravo 交付报告 — DK-19 每日笔记自动创建（core+tauri 面）

> 卡面即任务书（R-03 建卡：Assignee 建议 Bravo 事件/投影 Rust 面 + DoD 齐全）· 领地 write_path/event_bus/projection/desktop tauri/bootstrap 测试 · 预算 3–5 人日内

## 一、卡面四任务执行

1. **DailyNoteOpened 事件 + 投影消费**：AppEvent::DailyNoteOpened{date, note_id}（High 通道）；DailyNoteProjection（水位线 + apply 索引校验自愈 + rebuild 全量重灌——复用投影先例全套 trait 面）；
2. **幂等自动创建**：ensure_daily_note(ctx, date)——去重键=daily:{date} 索引 + NoteRecord.daily_date（serde default 兼容存量）+ scan 兜底自愈 + **AppCore.daily_note_lock 进程内互斥**（防并发双建）；行为级断言：**同日 5 路并发仅 1 篇、仅 1 路 created**；
3. **模板**：daily_template KV + {{date}}/{{weekday}} 渲染（中文星期，非法日期回退星期日）；
4. **设置开关**：DailyNoteMode Auto（默认开）/ Manual（仅定位不创建）/ Off（拒绝）；tauri set/get 命令。

## 二、诚实化与边界

- **workspace_id 裁剪**：单工作区口径（去重键=日期），多工作区立项时扩 daily:{ws}:{date}——与 S3 workspace 裁剪同口径；
- **UI 面 = Alpha**（今日页入口/设置开关 UI）；推送式"当日笔记实时通知"未做（拉模式——打开视图时 ensure）。

## 三、⚠️ 对向审核发现：writerchurn 稳定回归（非 flaky）

**发现**：`bootstrap_is_idempotent_across_restarts` 双 boot 必 LockBusy——**writerchurn（b657e77）writer 常驻持锁，first 未 drop 时 second boot 创建 writer 必失败**。Alpha d1ad2bc 的「Test exit101 flaky 判读」应修正：**这是稳定回归**（证据：测试结构 first 活到测试尾 + writer 常驻改造时序吻合 + 本地复现 100%）。
**处置**：最小修 bootstrap 测试——`drop(first)` 后再 boot（幂等断言语义不变：DEK 重用验证持久化恢复，非双实例并存）；**跨领地声明**：仅动 bootstrap 测试函数体，search.rs/writer 未触碰。
**留给 Alpha**：writer 惰性创建（首次 index 时才建）可根治双实例场景（桌面 dev reload 等），是否立项请裁决。

## 四、验证矩阵

| 门 | 结果 |
|---|---|
| dk19_daily | ✅ **5/5**（并发幂等/模板渲染/跨重启唯一可定位/模式语义/投影自愈） |
| bootstrap lib / core lib / mobile-ffi | ✅ 6/6（含回归修复后 idempotent）+ core 全绿 + 17 |
| clippy -D warnings（core/bootstrap/mobile 三包） | ✅ **0** |
| fmt 四包 | ✅ 净 |
| desktop 假pc check（tauri 可达） | ✅ 0（假 pc 已重建 /tmp/fakepc——Alpha 旧目录被清） |

**desktop clippy 附注**：全包 clippy 另有 4 处报警，全部归属 Alpha 既有面（ai_commands needless borrow/status.clone / import_commands 双处 struct update no effect / lib.rs:334 历史 doc 注释空行）——非 DK-19 引入，按领地纪律未动，建议 Alpha 顺手清偿。

## 五、tauri 面

cmd_daily_note_open（主入口：Auto 创建/Manual 定位/Off 报错）+ set_mode/get_mode + set_template/get_template，共 5 命令已注册。

## 六、CI

push 后回填。

— Bravo 2026-10-02（DK-19）
