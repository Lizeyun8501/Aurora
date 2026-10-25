# bravo-request：R17 FSRS 统计面（Phase 5 前置——revlog + 聚合 + 面板组件）

> 派单：Alpha ｜ 2026-10-25 09:55 ｜ 优先级：P1 ｜ 估时：4-6h ｜ Labels: `fsrs`, `area/core`, `P1`

## 背景

R13-R16 已闭环：队列持久化（snapshot_json/restore_json）→ 列表项卡化 → 复习入口正门（ReviewEntryCard + cmd_review_due_count）→ 大纲面板。FSRS 深化（原 Phase 5）候续首位——**统计面**。当前 ReviewQueue 只有 `stats() -> (due, total)` 两元组，**评分历史零记录**（revlog 缺失），留存率/今日复习量/到期分布全部无数据源。

## 勘察结论（Alpha 实证 2026-10-25）

| 点 | 实锤 |
|---|---|
| revlog 缺失 | `crates/aurora-core/src/l3_domain/fsrs.rs` L243-246：`ReviewQueue { scheduler, items: RwLock<Vec<ReviewItem>> }`——无评分日志字段；`review_card`（L274）只更新 state/due |
| 快照无日志位 | `snapshot_json`（L341）/`restore_json`（L365）手写序列化 items；version 迁移位已有（R13 坏条目跳过语义） |
| CardState 齐全 | L48-59：stability/difficulty/last_review/reps/lapses——state 分桶有数据基础 |
| stats() 现状 | L295 `-> (usize, usize)`（due, total）——消费方：due_items 徽标链（勿改签名，新增方法并列） |
| 挂载点 | `apps/desktop/src-tauri/src/lib.rs` cmd_review_due_count（L1098 R15）旁——invoke_handler 注册列表 L226 区 |

## 任务

### T1：ReviewQueue 加评分日志（core）

1. `ReviewLogEntry { card_id: String, rating: Rating, ts: DateTime<Utc> }`（derive Clone/Serialize——Rating 已有序列化面就沿用）
2. `ReviewQueue` 加字段 `review_log: RwLock<Vec<ReviewLogEntry>>`
3. 追加点：`add_card` 记首评（L257 尾）；`review_card` 评分成功才记（L274 Option 分支内——None 不记）
4. **容量纪律**：日志无界增长防爆破——`review_log` 上限 10_000 条，超限丢最旧（环形语义，注释诚实边界）

### T2：统计聚合（core）

`pub fn review_stats(&self, now: DateTime<Utc>) -> ReviewStats`（与现有 stats() 并列，不改旧签名）：

```rust
pub struct ReviewStats {
    pub total_cards: usize,
    pub due_now: usize,
    /// 今日已评分次数（revlog 本地日界——与 due_items 同用 Utc 会计口径偏差可接受，注释注明）
    pub today_reviews: usize,
    /// 今日 Again 占比（今日 reviews==0 → None）
    pub today_again_rate: Option<f64>,
    /// 未来 7 天到期分布（index 0=今日，[n] = 第 n 天 24h 窗口内到期数）
    pub due_7d: [usize; 7],
}
```

### T3：快照扩展（core）

- `snapshot_json` 加 `"review_log"` 字段；`restore_json` 对缺失该字段的旧快照**默认空**（R13 迁移位语义延续——不 bump version，字段缺省即兼容）
- 现有 roundtrip/坏快照单测**同步扩展**断言（日志 roundtrip + 旧快照兼容）

### T4：命令 + 面板组件（desktop，**不含挂载**）

1. `cmd_review_stats() -> ReviewStatsDto`（lib.rs，cmd_review_due_count 旁；serde camel?——沿用现有 DTO 命名惯例，对齐 ReviewCard snake_case 面即可）+ invoke_handler 注册
2. **新文件** `apps/desktop/src/components/ReviewStatsPanel.tsx`：
   - props `{ invoke: InvokeFn }`（InvokeFn 从 './DesktopShell' import type——现有组件同款）
   - 四卡：总卡数/今日到期/今日复习/今日 Again 率（None 显「—」）+ due_7d 迷你条形（div 高度映射，纯 CSS 零依赖）
   - 风格对齐 ReviewEntryCard（tokens.design 系；hot 态紫 #c9a9ff）
   - useEffect 拉 cmd_review_stats；失败静默 0 化（非阻断）
3. **禁碰 `DesktopShell.tsx`**——面板挂载（InfoPanelTabs 第 6 tab 或 ReviewPanel 内嵌）由 Alpha 在本任务书之外接线（域隔离：你交付组件导出 default，挂载点我来）

### DoD

- [ ] revlog 追加语义单测（add/review/None 不记/超限丢最旧）
- [ ] 快照 roundtrip 含 review_log + 旧快照（无字段）restore 兼容
- [ ] review_stats 聚合单测（due_7d 窗口边界：第 0 天=今日、第 6 天尾）
- [ ] cargo test -p aurora-core 全绿（474 基线+新增）+ clippy --all-targets -D warnings 0
- [ ] desktop tsc 0（ReviewStatsPanel 独立文件类型过）
- [ ] 接口签名零破坏（stats()/due_items() 原样）

### 验证门

本机 GTK 环境无 desktop cargo test 面——按既有惯例：core 面单测全绿 + clippy 0 + tsc 0 为交付证据链；回执文件本目录 `bravo-R17-FSRS统计面回执.md`。

## 域边界（零冲突纪律）

| 面 | 归属 |
|---|---|
| fsrs.rs / lib.rs 新命令 / ReviewStatsPanel.tsx 新文件 | **Bravo** |
| DesktopShell.tsx（大纲实时化 + 面板挂载） | **Alpha**（本批） |
| 共享层 shared/ui-components | 双方**均禁**（R17 无涉） |
