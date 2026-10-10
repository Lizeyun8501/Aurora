# DK-47 Alpha 验收注记（关账）

**验收**: Alpha 2026-10-10 19:35 ｜**结论**: ✅ 通过关账 ｜**commit**: fbf6ddd（单发一次成型，零 CI 修复红）

## 终证渠道

- **CI 五检查全绿**（fbf6ddd）：Test / MSRV / desktop-check / Clippy / Rustfmt

## DoD 五条核对（代码级精读）

| DoD | 结论 | 依据 |
|---|---|---|
| 1. 双用户隔离 | ✅ | dk47_dual_user_isolation：user-a 写 → user-b 读不见 → default 段亦隔离 |
| 2. 旧数据零丢失 | ✅ | dk47_legacy_lazy_migration（legacy 构造→迁移→内容一致+旧 key 已删=搬迁非复制）；**外加 persist 路径迁移读**——防 legacy 存量被「首次全量初始化」分支覆盖丢失 |
| 3. default 零回归 | ✅ | AppCore builder 缺省 DEFAULT_USER_ID；dk40a_tests 全量适配 DEFAULT 透传语义不变；对拍 isomorphic_write_path 双端 key 形态一致 |
| 4. 迁移幂等 | ✅ | dk47_migration_idempotent：二次读内容一致；旧 key 已删天然幂等 |
| 5. CI 全绿 | ✅ | 五检查全绿 |

## 设计亮点

1. **AppCore.user_id 字段方案**（非 WriteContext 扩字段）——WriteContext 零签名变更，业务主链从 `&core.user_id` 取，一个 core 实例=一个用户会话，最小侵入
2. **helper 双端唯一构造点**（app_core::notesnap_key）——desktop/mobile/测试全部走 helper，禁各自 format（dk02_s1_trash 适配也走 helper，纪律好）
3. **persist 路径迁移读**——Bravo 对 Alpha「get 单点」设计的正确补全（见下）
4. 常量锚定测试（LEGACY_NOTESNAP_PREFIX = "notesnap:"）防前缀误改

## 一处如实记录：禁触面划错，Bravo 判断正确

开工令 §4 将 write_path.rs 列禁触——**系 Alpha 勘察疏漏**（`rg -l "notesnap:" | head -5` 截断，漏了 write_path.rs 内三处快照读写：compact_update_log / load_or_init_doc / persist_doc）。Bravo 触碰的是**卡的靶心**（快照层 key 带段）而非禁触本意（write_path 主链业务逻辑/元数据层 NoteRecord 均未动），未触发 §5 停手条件（user_id 穿透的是快照函数签名，非 note:{id} 元数据层）。裁决：**触碰正确，验收通过**。

**流程教训**：禁触面划定必须全量列举 key 使用点（rg 结果不得 head 截断）——已入 Alpha 记档。

## 流水线收官

**四案排序 2→1→3→4 全部关账**（DK-44/45/46/47）。V1 阶段挂账卡清零（bench 落表除外——另行待 token 排期）。
