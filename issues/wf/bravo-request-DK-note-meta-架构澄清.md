# bravo-request: DK-笔记属性 S1 架构澄清（开工前裁决请求）

**发现时机**：开工侦察（10-27 22:40）。任务书设计基线与代码现状实质冲突——按「接口不够用→写 request 不硬改」惯例停工请求裁决。

## 冲突实锤（三处）

1. **任务书：「走 Note.metadata 现成字段（tags/icon/cover 三键）」**
   → **`NoteRecord` 无 `metadata` 字段**（write_path.rs:127 起平铺：id/title/content/…/tags）。
   `metadata` 现成字段不存在——任务书架构描述的起点不成立。

2. **tags 已有权威源+完整生态**（DK-27，write_path.rs:275 `set_note_tags`）：
   - KV `NoteRecord.tags` 平铺键=轻量读取源（TagsProjection / FilterRule `tags_include`/`tags_exclude` 消费中）
   - 写侧发 `NoteMetadataChanged` 事件（投影联动）
   - 诚实化注记在案：「Loro doc meta 双写暂缓——tags 权威源=KV NoteRecord」
   - **既有签名 `set_note_tags(core: &AppCore, note_id, tags, seal)` 与任务书签名 `(ctx: &WriteContext, note_id, tags)` 同名不同参**

3. **icon/cover 无落点**：任务书要求 serde default 零 migration——但新键落 `NoteRecord` = 碰 write_path.rs struct 定义（任务书停手线明文「write_path.rs 勿动」）。

## 三方案（预写死边界+预估）

### 方案 C'（推荐）：平铺沿 DK-27 惯例，note_meta.rs 做门面+校验层

- `NoteRecord` 新增 `icon: Option<String>` / `cover: Option<String>` 两平铺字段（`#[serde(default)]`——与 parent_id/kind/daily_date 同款惯例，存量零迁移达成任务书语义）
- tags **沿用 DK-27 既有键**（权威源/投影/事件全保留）；`note_meta.rs::set_note_tags` = 校验层（trim/≤32/≤16/去重保序/控制字符拒）→ 委托 write_path 既有函数
- **碰 write_path 的面**：struct 加 2 字段 + `put_note_meta` 无需改（serde 自动）——最小机械碰 ~6 行，注记披露请你复核
- 校验后 tag 变化继续发 `NoteMetadataChanged`（DK-27 投影联动不倒退）
- 预估：API 签名照任务书定死面不变；测试 12+ 例可全覆盖
- **风险**：struct 定义碰非领地（任务书停手线字面违反，需你特批）

### 方案 B：KV 独立键 `note:meta:{id}`（零碰 write_path）

- icon/cover/tags 全存独立 KV JSON 键，note_meta.rs 全自持
- **零碰 write_path** ✓ 字面合规
- 代价：①绕过 `put_note_meta` 串行编排（任务书要点 1「写回必须走 write_path 现有写编排」自我违反）；②tags 双源分裂（NoteRecord.tags 旧数据 vs 新键——投影/智能文件夹读旧键全看不到新写）；③读侧两跳
- 预估：实现快但技术债+数据分裂

### 方案 A'：metadata 子对象三键（任务书字面）

- NoteRecord 加 `metadata: NoteMetaJson` 子对象
- 代价：tags 从平铺键迁进子对象——**存量 tags 数据需 migration（违反任务书「零 migration」承诺）**+DK-27 投影/FilterRule 读点全改——波及面最大

## 请求裁决

- 若批 **C'**：我即开工（API 面/校验清单/DoD 全按任务书执行，仅落点形态=平铺非子对象+事件发布保留）
- 若批 **B/A'**：请明示，按批注执行
- 附加：`set_note_tags` 同名双签名问题在任何方案下都存在——建议 note_meta.rs 门面层命名保留任务书签名（模块内导出），write_path 旧函数保持内部（S2 桥面只接 note_meta.rs 新面）

——bravo 10-27 22:45
