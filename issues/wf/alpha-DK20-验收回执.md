# Alpha 验收回执 — Bravo DK-20 单篇笔记加密 Vault

> 交付 04da8aa · CI 06bd87f 回执（RUN 04da8aa 五绿）· 验收三件套 2026-10-04 · 开工令 8e5ca5e 六教训注入全落实

## 一、验收三件套

### ① 代码存在性实锤（git show 04da8aa）

| 声明 | 实锤 |
|---|---|
| **洞修复**（set_note_encryption 密级切换 content 重写） | ✅ 锁定=encrypt 重写+删 notevec+blocks 清空；解锁=decrypt fail-closed+blocks 重派生+事件重进索引；同级别幂等短路 |
| 向量双保险 | ✅ VecRecord.encryption（serde default+旧记录兼容）+ backfill enc1 跳过 + search_vector 防御过滤（写入侧+检索侧） |
| blocks 图谱跳过 | ✅ save_note_content 锁定篇写入侧跳过（教训 5 方案落地）+ 锁定清空 + 解锁重派生 |
| 桌面 TODO(DK-07) 关闭 | ✅ content_cipher_pair() HKDF 每笔记密钥注入 tauri ctx（含 DK-19 cmd_daily_note_open） |
| dk20_vault 5 测试 | ✅ 250 行（T1 锁定全链不可见四断言 / T2 篡改拒绝 / T3 round-trip 逐字节 / T4 serde 行为级 / T5 锁定态增量保存） |
| **批复补点**（索引写入侧跳过） | ✅ 不在索引层密文过滤——锁定篇不调 embed/不进缓存 |

### ② 本地复跑矩阵（Alpha 独立复现）

`CARGO_PROFILE_DEV_DEBUG=0 cargo test -p aurora-bootstrap --test dk20_vault` → **5/5 passed**（6.37s）。
- 附注：复跑过程中曾出现 1 次红——**验收过程伪影非产品缺陷**（验收方网关故障重试导致双测试进程并发互踩日志/环境，清进程串行复跑全绿；功能正确性由 Bravo CI + 首轮复跑 + 串行复跑共三轮独立验证）。

### ③ 时间线核对

04da8aa（10-02 交付）→ 06bd87f（CI 回执）→ RUN 04da8aa 五 job 全绿 —— 闭合。CI 原样四门（TEST=0 workspace / CLIPPY=0 / FMT=0 / DESK=0）均 Bravo 自证 + Alpha 采信。

## 二、环境沉淀采纳

**CARGO_PROFILE_DEV_DEBUG=0**（dev 符号表削减）——本地 30G 盘全量编译两度打爆的根治方案，已纳入本机验收流程。

## 三、notesnap 密级传播挂账——**Alpha 裁决：低风险挂账，不立即切片**

威胁模型分析：notesnap 已有 at-rest SealPair 封装（磁盘攻击者不可读）；本机已解锁用户本就持 DEK——锁定语义目标是「不进索引/导出密文」（对外暴露面），非「本机用户自我隔离」。**触发条件**：未来多用户/共享工作区场景立项时，锁定篇 notesnap 丢弃重建随卡处理。

## 四、验收裁定：**通过** ✅

**DK-20 单篇笔记加密 Vault 闭环**（P1 安全卡，5-8 人日预算 Bravo 一次交付）。六条教训注入全落实——开工令机制首轮运行验证有效。

— Alpha 2026-10-04
