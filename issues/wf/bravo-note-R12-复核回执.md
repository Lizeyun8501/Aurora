# bravo-note：R12 对向复核回执（块卡同步语义）

> Bravo ｜ 2026-10-24 午 ｜ Alpha 域批——复核八连

## 四路语义全验 ✓
- **他笔记不动**：retain 首门 note_id 过滤 ✓
- **失效清队**：`!(is_block ∧ ¬keep)` 布尔代数精确——笔记级卡永不误清、在 keep 块卡保留 ✓
- **FSRS 进度保留**：已存在 continue，scheduling 零扰动 ✓
- **缺失注册首评 Good**：new_card→review(Rating::Good) 真落调度（非裸建卡 state），与声明一致 ✓
- 锁纪律：write() 单次获取全程持锁（retain+push 原子）；scheduler.review 无重入锁（472 绿实证无死锁）

## 单一事实源三层锚定 ✓
core fsrs.rs L348 is_block_card_id ↔ desktop lib.rs L1014 due 判定 ↔ TS 正则 /:b[0-9a-f]{8}$/（core 注释互指）——R10 手写双实现正式收敛，跨层语义漂移面封死
- 单测三件（形态判定/进度保留/清理幂等）与 commit 声明一致 ✓

## 过程自纠（教训新变体入库）
`rg -rn` 非 recursive——是 **--replace "n"**！`fsrs::n`/`nnew()` 全为伪影（is_block_card_id/pub fn 被吃）——差点误判「单字母函数名反常」。rg 默认递归无需 -r；匹配文本输出一律核 -r 伪影。

## 全局态
块卡链路 v2（同步语义）收官：重锚四路全绿+三层单源。复核八连零新增 P3。
