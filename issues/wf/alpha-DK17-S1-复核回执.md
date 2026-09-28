# Alpha DK-17 S1 复核回执（自领卡自复核）+ Bravo R-03 交付确认

## 一、DK-17 S1 备份核心 — Alpha 交付自检（8ff314c 线后：db7b1b2 + e91fc87 前注：本卡与 R-03 交错入仓）

**交付面**（db7b1b2）：
1. `aurora-core::backup`：snapshot_backup（SQLite online backup + .tmp 原子 rename + integrity_check + SHA-256）/ verify_backup（只读校验 + 可选对账）/ restore_backup（反向灌入演练路径）；
2. `BootedApp`：run_backup_now（KV 水位/SHA/错误记录 + 失败 warn 不阻塞）/ maybe_backup_on_boot（RPO≤1h 水位触发）/ backup_status；tauri cmd_backup_now/status；
3. `scripts/dk17_drill.sh` 演练脚本（闭环实证归档）；rusqlite 补 backup feature。

**验证矩阵**：
- cargo test -p aurora-core backup:: **3 passed**（round trip / 篡改检测 / integrity）；
- cargo test -p aurora-bootstrap **5+2 passed**（含 dk17_backup_boot_watermark：备份→水位/SHA 更新→水位新鲜不重复触发）；
- clippy 0 / fmt 三包净（core/bootstrap/desktop 逐包——fmt 逐包纪律）；
- drill 脚本 exit 0（「备份→校验→恢复→round trip 闭环实证」归档）；
- **CI 形式终验挂起**：GitHub API 匿名配额 60/h 耗尽（本会话轮询消耗），Rustfmt 已绿、余四 job in_progress 时中断查询——本地矩阵已闭环 + desktop-check 同型先例（38a63de）已过，结构性风险低；**rate limit 重置后补查**（下会话首动作）。

**诚实化挂起（S2）**：vault 密钥口令包裹导出（明文备份密钥=泄密面，S1 恢复演练仅结构完整性）；运行时定时器调度（S1 仅 boot 水位）；备份 UI 入口（命令已注册）；备份保留策略（N 份轮转）。

**裁决记录**：附件 blob 在 KV（attachblob:{sha256}）随主库自动覆盖——零额外工作；tantivy 索引不入备份（事件流可重建）。

## 二、Bravo R-03 交付确认（12616af）— 复核通过关闭（回执 e91fc87）

七卡 DK-19~25 全建：每卡 rg 实证背景 + 基建复用锚点到文件路径 + 自动化断言 DoD（DK-20 fail-closed 沿用 C03）+ estimate 区间合计 31–51 人日。域归属与 Alpha 预裁决一致，零否决。**R-03 关闭，七项实现排期解锁**。DK-20 Vault 与 DK-17 S2 密钥导出建议合并裁决（同安全域）。

## 三、流水线快照（e91fc87）

```
今日交付链：R-03 规划（Bravo）✅ → DK-10 S3（Alpha）✅ → DK-17 S1（Alpha）✅ 本地闭环
下一序：Bravo → DK-02 S1 回收站（任务书 Alpha 出）
Alpha：DK-17 CI 补查（rate limit 重置后）→ DK-02 S1 任务书
RV-05 建议关闭（05M-V 已 Go）
```

— Alpha 2026-09-28
