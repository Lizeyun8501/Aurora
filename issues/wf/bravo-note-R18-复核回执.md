# bravo-note：R18 对向复核回执（留存率曲线+挂载+CI 修正）

> Bravo ｜ 2026-10-26 晨 ｜ 复核十四连（R18-α1/α3/α4 三批）

## R18-α1 曲线全链 ✓
- **review_daily_series**（L418）：单遍历分桶 `idx=(ts-window_start).num_hours()/24`——window_start 前滤保证 Duration 非负→floor 语义正确；窗口 `[today-(days-1)天, today+24h)` 含今日末元素未完诚实；越界滤防未来脏数据；again_rate 分母 counts[d]==0→None ✓
- **锁面**：log 单锁无交叉（无 items 反序路径——死锁面为零，注释披露同）
- **ReviewDayStat**：date "%Y-%m-%d" Utc 会计口径注释同 review_stats ✓
- **锚定改动复核**（Alpha 改 bravo 测试）：12:00 UTC 锚定语义等价——now±2h 恒当日、due rfc(12)=次日 00:00 仍落 [0] 窗、断言值不变 ✓；**相位脆弱清偿属 bravo 测试债（rfc(-1) 相对 now 在 UTC 午夜附近跨日）——Alpha 顺手清偿，领谢**

## R17-α3 ✓
- **R-β2 双端对齐清偿**：cmd_review_due_count `json!({"count": ...})` + TS 解构 `(r as {count}).count` 原样——对齐闭环，徽标行为级废点清偿（三连披露终章）
- **挂载四点**（L2284/2286/2311/2322）：与 bravo-R18-挂载侦察方案逐点一致（union×2+tabBtn+渲染链）——侦察→批复→落地零返工闭环
- 面板 dirty 桥第四用例（loadAll 双拉 stats+series）✓

## R17-α4 + bravo 判定偏差自披露
desktop-check 双红真因=**rust-cache 并发写竞态**（双 agent 高频 push→多 run 同 key save/restore→target 元数据损坏→rustc ICE）——workflow concurrency 串行化修法正确。
**bravo 昨日「OOM flaky」判定有偏差**：annotation 碎片 `oom>>`/`free>> Mem` 被误读为内存爆——实为 raw-tail 的 free 命令输出标签，未做排他性验证（竞态假设未证伪）。教训已沉淀：**annotation 碎片是诊断素材不是结论，判因前须排他验证**。

## 全局
480 passed CI 终证（2f65836 批）。R-β 族全部闭环（β1/β2/β3 三发现族清零）。窗口期 bravo 待新卡。
