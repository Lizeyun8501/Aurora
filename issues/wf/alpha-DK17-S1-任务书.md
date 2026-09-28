# Alpha 自领任务书 — DK-17 S1：备份核心（RPO≤1h 快照 + 完整性校验）

> 派发对象：Alpha（排期计划并行第二线自领）· 2026-09-28 · 基线 d6bf946
> 预算：8 人日 · 领地：crates/aurora-core/src/backup.rs（新）+ bootstrap/tauri 接线面

## 一、范围（S1 收敛）

1. **snapshot_backup**：SQLite online backup（rusqlite 0.32 bundled 自带 backup 模块）→
   `data_dir/backups/aurora-backup-<ts>.db`（先写 .tmp 再原子 rename）；产出
   BackupReport { path, sha256_hex, size_bytes, duration_ms }；
2. **verify_backup**：只读打开 + `PRAGMA integrity_check` + SHA-256 对账；
3. **restore_backup**：备份文件 → 目标库反向灌入（演练路径）；
4. **BootedApp 接线**：`maybe_backup_on_boot`（KV `settings:backup.last_ts` 距今 >1h 触发，
   **失败仅 warn 不阻塞启动**）+ `backup_status()`；tauri `cmd_backup_now / cmd_backup_status`（UI S2）；
5. **演练即测试**：round trip（写→备→清→恢→一致）/ 篡改检测（sha 对账失败）/ integrity 三单测 +
   scripts/dk17_drill.sh 包装脚本。

## 二、关键裁决（冻结）

- **附件零额外工作**：blob 在 KV `attachblob:{sha256}`（DK-09 内容寻址），随主库快照自动覆盖；
- **tantivy 索引不入备份**：可从事件流重建（projection 既有语义）；
- **vault 密钥备份挂起 S2**：明文备份密钥=泄密面；S2 裁决口令包裹（PBKDF2）导出——S1 恢复演练
  到不含 DEK 的库仅验证结构完整性，标注「密钥未恢复」；
- **调度降级**：S1 仅 boot 时按水位触发（无后台 tokio interval——避免调度复杂度），小时级 RPO 由
  「应用启动频率 ≥ 每小时」的桌面使用假设支撑，S2 补运行时定时器；
- **告警**：校验失败 → tracing error + KV `settings:backup.last_error`（UI 通知 S2）。

## 三、DoD

1. `cargo test -p aurora-core backup::` 三测试全绿；`cargo test -p aurora-bootstrap` 全绿；
2. clippy 0 + fmt 净（**逐包**——aurora-core + aurora-bootstrap + aurora-desktop 都动则都查）；
3. dk17_drill.sh 演练跑通输出归档（回执引用）；
4. CI 五 job 绿；挂起项（密钥/调度/UI）显式入回执。

— Alpha 2026-09-28
