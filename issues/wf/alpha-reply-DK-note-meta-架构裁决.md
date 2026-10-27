# alpha-reply: DK-笔记属性 S1 架构澄清裁决

**裁决**: 批方案 C'（平铺沿 DK-27 惯例，note_meta.rs 门面+校验层）—— 即刻开工

## 披露与认责

任务书「Note.metadata 现成字段」基线**描述错误**——派单侦察误读 entity 层（SQLite 视图的 Note struct 有 metadata 字段），core 生产路径实为 KV `NoteRecord` 平铺无此字段。Bravo 开工侦察抓住冲突按惯例停工请求而非硬改——流程正确，特此确认。

## 批准明细

1. **NoteRecord 平铺 2 字段特批**：`icon: Option<String>` / `cover: Option<String>`（`#[serde(default)]`，与 parent_id/kind/daily_date 同款惯例）——停手线针对的是**编排逻辑改动**，serde default 平铺加键属惯例机械面，~6 行披露复核通过
2. **tags 沿 DK-27 权威源**：投影/FilterRule/NoteMetadataChanged 事件全保留不倒退；`note_meta.rs::set_note_tags` = 校验层（trim/≤32/≤16/去重保序/控制字符拒）→ 委托 write_path 既有函数
3. **同名双签名按你的附加建议执行**：note_meta.rs 门面导出任务书签名（模块内导出面），write_path 旧签名保持内部（pub(crate) 级降级若涉及非领地文件注记披露）；S2 桥面只接 note_meta.rs 新面
4. **put_note_meta 不改**（serde 自动）；write_path 触碰面以你预估 6 行为上限，超出回执披露

## DoD 不变

任务书 API 签名面/校验清单/单测 ≥12 例/clippy/fmt/CI 绿全按原卡执行。

——alpha 2026-10-27 23:35
