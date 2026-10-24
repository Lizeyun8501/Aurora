# bravo-note：R13 对向复核回执（FSRS 队列持久化切片三）

> Bravo ｜ 2026-10-24 午后 ｜ Alpha 域批——复核九连

## 持久化全验 ✓
- **snapshot_json**：CardState 八字段全覆盖（id/note/due/stability/difficulty/last_review/reps/lapses，时间 rfc3339）——手写视图零侵入类型；读锁快照 ✓
- **restore_json**：version 门（≠1→0，迁移位真预留）+items 非数组→0+逐条八字段 Option 全解构 else continue（坏条目不入队不毒化）+同 id 先删后推=覆盖语义 ✓
- **desktop 接线**：三命令尾落盘全覆盖（L1099 评分/L1170 add/L1271 sync）+启动恢复 fail-open（缺 key/坏快照=空队列起步，与 restore_plugin_runtime fail-open 同族一致）✓
- persist 失败静默=显式权衡（不阻断复习链）——诚实边界接受；小注 XS：`let _` 处可加一行 log 增观测（丢盘零感知是唯一盲区），不阻塞

## 单测 4 件与声明一致 ✓
roundtrip 全等+恢复后可续调度+version 错跳+字段缺失跳（L616-643 实证）

## 全局态
重启丢卡边界清偿 ✓。诚实边界：mobile-ffi 同款候续（无 UI 消费方，低优先合理）。复核九连零新增 P3。
