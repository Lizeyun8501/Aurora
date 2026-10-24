# bravo-note：R15 对向复核回执（复习入口正门）

> Bravo ｜ 2026-10-24 夜 ｜ Alpha 域批——复核十一连

## R-β2：cmd_review_due_count 返回类型错配（行为级，tsc 盲区）⚠ 建议修
- **Rust**（lib.rs L1100）：`-> Result<usize, String>` 裸值直序 → JS 收**数字**
- **TS**（DesktopShell.tsx L775-776）：`(r as { count: number }).count` 解构对象 → 裸数字无 .count → **恒 undefined**
- **后果链**：due 恒 undefined → `due ?? '…'` 永显 '…'（加载中伪影）→ hot 恒 false → 紫框/热按钮永不激活 + 「暂无到期卡」误导文案（即使有 due 卡）——**正门徽标永久降级**
- **tsc 抓不到**：as 断言强转（类型说 number 实际 undefined）——CI 永远绿，R-β1（`!idx` 误杀）同族
- **惯例反证**：cmd_list_notes→as 数组、cmd_today_view_stats→as 对象均直序直收，无全局 wrapper
- **最小修二选一**：A）TS 单行 `setDue(r as unknown as number)`；B）Rust 返 `json!({"count": n})` 对齐解构。倾向 A（Rust 面零动）

## 其余全验 ✓
- cmd_review_due_count 真轻：due_items(now).len() 无卡面重拼（全量重拼留 cmd_due_review_cards 分工清晰）✓
- ReviewEntryCard：invoke 失败 catch→0 降级、0 张引导文案（建卡指引）、onOpen=setReviewOpen 复用 ReviewPanel、L4011 Today 挂载 ✓
- cmd 注册 L227 ✓；hot 态紫 #c9a9ff 与 R9 面板紫一致 ✓

## 全局态
五批 Distill 管线可见性正门落地（R-β2 修后即全通）。复核十一连：1 行为级发现（R-β2）。
