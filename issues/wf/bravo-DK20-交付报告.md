# Bravo 交付报告 — DK-20 单篇笔记加密 Vault

> 开工令 8e5ca5e · 六条教训注入全落实 · 领地 write_path/vector_search/desktop tauri/bootstrap 测试（vector_search 为跨领地最小补丁——教训 5 授权写入侧跳过方案）

## 一、卡面四任务执行

1. **encryption 字段落存储与迁移**：DK-07 S1 已有（"none"|"aes256gcm" + serde default）——本卡补**行为级反序列化断言**（T4：无字段存量 JSON → "none"，非仅注解）；
2. **Vault 密钥派生**：复用 DK-07 S2 NoteContentCipher（HKDF-SHA256(note_id) 每笔记独立 256-bit 密钥，独立于工作区 SealPair）——本卡关闭桌面 TODO(DK-07)：**content_cipher_pair() 装配注入全部 4 处 tauri ctx 构造**（含 DK-19 的 cmd_daily_note_open）；
3. **锁定语义全链**：
   - **洞修复（核心）**：旧 set_note_encryption 只改字段不重写 content——明文笔记"锁定"后仍明文落库。现：**锁定 = encrypt 重写 + 删 notevec 向量残留 + blocks 清空**；**解锁 = decrypt 重写（fail-closed：校验失败拒绝、record 零变更）+ blocks 重派生 + 事件驱动重进索引**；同级别幂等短路；
   - **不进 FTS**：SearchIndexProjection source_entries 密级过滤（S3 已有）+ 锁定/解锁事件驱动 reindex 移除/恢复（行为级验证）；
   - **不进向量**：写入侧 backfill 遇 enc1: 前缀跳过 + VecRecord.encryption 字段 + search_vector 防御过滤（双保险）；
   - **不进图谱（blocks）**：save_note_content 锁定篇写入侧跳过派生 + 锁定时清空 + 解锁重派生；
   - **导出为密文**：导出路径 load_note_meta 直出 = enc1 密文（T1 字节级断言）；
4. **解锁交互**：core 面 = set_note_encryption(none)（主密钥校验 = decrypt 成功即通过，篡改拒绝）；UI 细节留 Alpha。

## 二、诚实化与边界

- **notesnap（Loro 快照）层未做密级传播**：锁定篇 notesnap 内仍是明文（有 SealPair at-rest 整体封装保护，但 unseal 后应用层可读）——与 rec.content 的 enc1 保护差一层。**挂账 Alpha 裁决**：独立切片（锁定时冻结/删快照+解锁重建）成本较高，本卡 DoD 三条不涉及；
- 向量路无 embedding provider 时 backfill 不触发（desktop 未接 Ollama）——防御过滤为前置布置。

## 三、验证矩阵（CI 原样命令，开工令教训 2）

| 门 | 结果 |
|---|---|
| `cargo test --workspace --exclude aurora-desktop` | ✅ **TEST=0**（dk20_vault 5/5 + dk19 5/5 + dk02 15/15 + 全量回归含 bootstrap/core/mobile-ffi） |
| `cargo clippy --workspace --exclude aurora-desktop --all-targets -- -D warnings` | ✅ **CLIPPY=0** |
| `cargo fmt --all -- --check` | ✅ **FMT=0** |
| `PKG_CONFIG_PATH=/tmp/fakepc cargo check -p aurora-desktop` | ✅ **DESK=0**（假 pc 已重建落 /tmp，已记 daily 防丢） |

**环境韧性记录**：本地 30G 盘两次被 workspace 并行链接打爆（ld Bus error signal 7）——根治：cargo clean + `CARGO_PROFILE_DEV_DEBUG=0`（符号表削减，CI 无此问题）；clippy 首跑 `-j` 误传 clippy-driver 已修正位置。

## 四、DoD 三条断言映射

- 锁定后搜索/图谱/导出不可见明文 → **T1**（FTS 零命中 + blocks 空 + notevec 无 + KV 字节非明文 + content 无原文）；
- 密文校验失败拒绝 fail-closed → **T2**（篡改 hex nonce → open_note_content Err；非法级别 InvalidInput；同级别幂等）；
- 解锁 round-trip 逐字节一致 → **T3**（content == 原文逐字节 + 恢复可检索 + blocks 重建）。

## 五、CI

push 后回填。

— Bravo 2026-10-04（DK-20）

## 五、CI 终验（回填）

RUN 04da8aa = **SUCCESS**（五 job 全绿）。DK-20 单篇笔记加密 Vault 闭环。
⚠️ notesnap 密级传播挂账待 Alpha 裁决（独立切片）。

— Bravo 2026-10-04
