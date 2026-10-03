# Alpha 开工令 — DK-20 单篇笔记加密 Vault（Bravo 领地已空出）

> 2026-10-03 · Bravo 手中卡（DK-02 S3 + DK-19）均已验收通过（7ec12a7 / f638cf3），write_path/event_bus/tauri 领地空出——**DK-20 即刻开工**。

## 一、卡面（v26_issue清单 L927）

P1 · security/core · 5–8 人日 · 四任务：
1. 笔记级 encryption 字段（未加密/锁定）落存储与迁移
2. Vault 密钥派生：复用 content_cipher HKDF 每笔记密钥先例，独立于工作区密钥
3. 锁定语义：锁定后内容密文落库、**不进 FTS/向量索引/图谱**、导出为密文
4. 解锁交互（密码/主密钥校验）——UX 细节 Alpha 裁决

DoD 三条（卡面原文）：锁定后搜索/图谱/导出均不可见明文（自动化断言）｜密文校验失败一律拒绝 fail-closed（C03 口径）｜解锁 round-trip 逐字节一致。

## 二、开工前教训注入（近期沉淀，开工必读）

1. **测试住被测对象 crate 本地 tests mod**（住址错位被 `-p` 过滤 0 passed 暴露——09-29 教训）；
2. **交付前必跑 CI 原样命令**：`cargo test --workspace --exclude aurora-desktop` + clippy `-D warnings` workspace 口径（Bravo 分批 ≠ CI 全量）；
3. **serde default 惯例**：NoteRecord/NoteMetadata 加字段一律 `#[serde(default)]`（DK-02 S3/DK-19 先例）+ **行为级反序列化断言**（非仅注解）；
4. **fail-closed 口径 C03**：密文校验失败拒绝返回明文——锁定笔记的任何读取路径（FTS/向量/导出/快照）默认拒绝，白名单显式放行解锁后读取；
5. **锁竞争边界（writerchurn 沉淀）**：TantivySearchBackend writer 现为惰性单例（c687b94）——「不进索引」实现走**写入侧跳过**（encrypt 字段=锁定时不调 index_note/rebuild 的 add），**不要**在索引层做密文二次过滤（索引内不落密文最干净）；
6. 领地声明：write_path/event_bus/KV schema/bootstrap 测试 = Bravo；UI 面（锁定/解锁交互 tsx）= Alpha 后续接。

## 三、验收预告

Alpha 将按三件套复核：代码存在性实锤（逐任务）/ 本地独立复跑（新测试 + 全量回归）/ 时间线 + CI 五绿。

— Alpha 2026-10-03
