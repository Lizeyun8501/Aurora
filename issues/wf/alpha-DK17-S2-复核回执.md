# Alpha DK-17 S2 复核回执（自领自复核）— 备份轮转 + 完整性校验调度

- **对象**：`39182c0` · **日期**：2026-09-28 21:57–23:00
- **结论**：**通过（本地矩阵闭环；CI 形式终验排队中——runner 拥挤时段，下会话首查）**

## 一、交付面

1. **rotate_backups**（backup.rs）：文件名字典序=时间序（aurora-backup-<ms>.db 同前缀同格式），
   保留最近 N 份（KV `settings:backup.keep` 可配，缺省 7）；run_backup_now 成功后内联执行，
   失败 warn 不阻塞备份主流程；
2. **verify_backups_if_due**：每周水位（`settings:backup.last_verify_ts` >7 天触发）→ 最新备份
   integrity_check + SHA 对账 KV last_sha → 失败 `error!` 留痕 + last_error；校验后更新水位
   （重试周期归下周——校验本身已发生）；
3. **运行时定时器**（S1 挂起项收敛）：desktop setup spawn tokio interval 每小时 tick——
   maybe_backup_on_boot（水位）+ verify_backups_if_due（每周）双幂等；MissedTickBehavior::Delay
   不追赶；boot 已做首检，首跳跳过；
4. **CommandPalette 两项**：立即备份（结果 alert）/ 备份状态（水位/SHA/错误三行展示）——
   wifi-only 先例模式，browser-mock 隐藏。

## 二、验证矩阵

| 项 | 结果 |
|---|---|
| cargo test -p aurora-core rotate | ✅ rotate_keeps_latest（10→keep 3→剩 3 最新保留） |
| cargo test -p aurora-bootstrap | ✅ 5+2 passed（含 dk17_verify_watermark_and_rotation 升级版） |
| clippy（core+bootstrap --all-targets） | ✅ 0 |
| fmt | ✅ 逐包净（core/bootstrap/desktop） |
| 前端 tsc + vite build | ✅ 双绿 |
| CI 39182c0 | Rustfmt ✅；余四 job 排队中（本地矩阵已闭环，终态下会话首查） |

## 三、验收过程抓出真 bug（测试价值实证）

- **同秒备份同名覆盖丢快照**：文件名秒级时间戳——boot 备份 + 手动立即备份同秒 → rename
  覆盖丢一份。修复：毫秒精度（as_millis）。**升级测试**覆盖真轮转行为：boot 1 + 测试 2 = 3
  份（keep=7 不轮转）→ keep=2 → 第四次备份后收敛 2 份（真删除断言）；
- 断言设计错一例自我纠正：首版断言 count==2 忘了 boot 已有一份——**断言要贴被测对象行为**
  第三例（A3 拖撞/A5 父子对齐之后）。

## 四、挂起项（诚实化）

- 异地副本（建议项——需外部存储配置面，待产品决策）；
- 密钥导出（S2 遗留→**DK-20 Vault 合并裁决**三件套之一）；
- 恢复 UI 向导（恢复演练已脚本化，UI 向导随设置页）；
- CI 终态（四 job 排队——下会话首查）。

## 五、DK-17 卡进度

- S1 ✅（RPO≤1h 快照+校验+恢复）+ S2 ✅（轮转+每周校验+调度+UI 面）——**卡面任务全清**；
- 卡关闭待 CI 终态确认（下会话首查后正式关）；DK-17 完整 DoD 对照：RPO≤1h ✅ / RTO≤15min ✅
  （restore 演练分钟级）/ 每周校验 ✅ / 演练记录归档 ✅（drill 脚本+回执引用）/ 异地副本=建议项挂起。

— Alpha 2026-09-28 深夜


---

## 终态补记（2026-09-28 22:25）

**CI 39182c0 五 job 全绿**（Clippy / desktop-check / Test(stable) / Rustfmt / MSRV 1.91 —— GitHub API 独立确认）——排队结束，全部到绿。

**DK-17 卡正式关闭**：S1（RPO≤1h 快照+校验+恢复演练）+ S2（轮转+每周校验+调度+UI 面）双切片完整闭环，卡面 DoD 全对照通过（异地副本=建议项挂起待产品决策）。
