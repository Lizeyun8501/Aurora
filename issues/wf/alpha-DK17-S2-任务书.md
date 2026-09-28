# Alpha 自领任务书 — DK-17 S2：备份轮转 + 完整性校验调度（独立切片）

> 派发对象：Alpha（自领）· 2026-09-28 22:00 · 基线 `8ce335c`
> 领地隔离声明：Bravo 占 l2_engines/write_path/traits（DK-03 S2 + DK-02 S2）——本卡只碰
  backup.rs / bootstrap 备份方法 / desktop setup / CommandPalette——**零交集**。
> 预算：2–3 人日

## 一、范围（S1 挂起项收敛）

1. **轮转策略**：`rotate_backups(out_dir, keep)`——按文件名时间戳排序保留最近 N 份
   （默认 7，KV `settings:backup.keep` 可配）；run_backup_now 成功后自动执行；
2. **每周完整性校验**：KV `settings:backup.last_verify_ts` 距今 >7 天 → boot 时对最新备份
   verify_backup（integrity + SHA 对账 KV last_sha）→ 失败 tracing error + last_error 留痕
   （「无演练的备份等于没有备份」DoD 缺口补上）；校验后更新水位；
3. **运行时定时器**（S1 挂起项收敛）：desktop setup spawn tokio interval（每小时）调
   BootedApp.check_and_backup()（水位判断复用 S1 maybe_backup_on_boot 逻辑）；
4. **UI 接入**：CommandPalette 两项——「立即备份」（cmd_backup_now + 结果 toast 文案）/
   「备份状态」（cmd_backup_status → alert 展示水位/SHA/错误）——wifi-only 先例模式；
5. **演练归档**：dk17_drill.sh 输出由回执引用归档（机制沿用）。

## 二、DoD

1. 单测：轮转（造 10 份 → keep 3 → 剩 3 且最新保留）/ 每周校验水位（旧水位触发 → 新水位
   不重复）/ tick 幂等（同水位二次调用不产生新备份）；
2. 全量 -p aurora-core -p aurora-bootstrap 不回退 + clippy 0 + fmt 逐包净；
3. CI 五 job 绿；挂起项（异地副本/密钥导出/恢复 UI 向导）显式列表。

— Alpha 2026-09-28
