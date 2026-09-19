# WF-Alpha 设计卡：DK-07 笔记级加密落库（分层厘清与实施路径）

> 2026-09-19 盘点结论 · Alpha 工作流 · 基线 4218f7c

## 1. 盘点发现（重要，纠正此前判断）

**「vault.encrypt 生产调用点为零」的判断不完整**——`write_path.rs` 已有完整 at-rest 加密链路：

- `SealPair { seal, unseal }`（write_path.rs:55）：桌面端注入 LocalDekVault 实现
- `put_note_meta` / `load_note_meta`：对称封装，payload = NoteRecord JSON **整体**加密后落 KV `note:{id}`
- 移动端 seal = None（明文落库）
- 结论：**桌面端全量信封加密已生效**（打开即全部笔记密文落盘）

## 2. 两层加密语义厘清（关键设计决策）

| 层 | 现状 | 语义 | 信任边界 |
|---|---|---|---|
| **at-rest 全量 seal**（已实现） | SealPair 整体加密 | 落盘防偷（硬盘丢失/文件窃取） | 本机运行时明文，Tantivy 索引明文是 V19 架构约定 |
| **笔记级 Vault**（DK-07 主体，未实现） | ❌ NoteRecord 无 encryption 字段 | 用户主动锁定某笔记：**锁定态连本机索引/搜索/图谱/导出都不可见** | 超出本机信任边界 |

**bootstrap source 交叉验证**：seal 开启时 bootstrap 直接 scan KV 得密文 bytes → JSON 反序列化失败 → filter_map 丢弃 → 索引空（fail-safe，无泄露但无索引）。

## 3. 实施路径（四个切片，按序）

### S1：NoteRecord 密级字段（本轮骨架）
- `NoteRecord` 加 `encryption: String`（serde default "none"，兼容存量 KV 数据）
- `put_note_meta`/`load_note_meta` 传播；新增 `set_note_encryption(note_id, level)` 写路径（经 WriteContext 事件总线发布 `NoteEncryptionChanged` 事件）
- 迁移：存量笔记默认 "none"

### S2：笔记级 seal/unseal（核心）
- LocalDekVault 之上建笔记级 KEK-DER 层：笔记密钥 = HKDF(master_dek, note_id)（每笔记独立密钥）
- `save_note_content` 分支：record.encryption == "aes256gcm" → content 用笔记密钥 AES-GCM 加密后入 record（record 其他字段仍受 at-rest seal 保护）
- 锁定态：内存笔记密钥表可清除（Lock All → 密钥清零 zeroize）

### S3：全链路密级传播
- bootstrap source：Rec 解析后依 `encryption` 字段 → IndexEntry.metadata.encryption = Encrypted（配合已落的投影三路过滤）
- 搜索/图谱/导出服务：Encrypted 且锁定 → 一律排除（搜索后端层过滤而非结果过滤，防侧信道）

### S4：锁定态 UI 与移动端
- mobile-ffi 桥：`set_note_encryption` / `lock_vault` 暴露
- WebView 锁图标 + 解锁口令流程（KeyHierarchy::unlock，见 key_hierarchy.rs 已有实现）

## 4. 依赖与接口冻结

- S1-S3 触达 aurora-core（write_path/app_core）+ aurora-security（S2 密钥派生）——**Alpha 领地内**，无跨 WF 冲突
- S4 触达 mobile-ffi + apps/mobile——与 DK-15（WF-Delta 无关，移端归 Alpha 集成窗口）
- 事件新增 `NoteEncryptionChanged` → AppEvent 枚举变更 → 周一冻结窗口集中合入（规约 §5）

## 5. 验收（DoD 映射）

- Private/锁定笔记：搜索零命中（S3 测试）+ 导出输出占位符 + 图谱节点隐藏
- 密文校验失败一律拒绝返回明文（fail-closed，复用 dk07_ciphertext_tamper 测试语义）
- 锁定后内存密钥 zeroize（复用 dk07_master_key_zeroize_on_drop 语义）
