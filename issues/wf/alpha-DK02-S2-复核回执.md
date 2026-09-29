# Alpha DK-02 S2 复核回执 — 目录树与原位还原（Bravo 交付 1b99eb6）

- **复核人**：Alpha · **日期**：2026-09-29 12:24–13:20 · **交付 commit**：`1b99eb6`
- **复核结论**：**有条件通过——交付面合格（架构/测试覆盖优秀），但抓出 1 个集成级真缺陷
  （CI Test ❌ 根因）+ 2 处 CI Clippy ❌ lint，已由复核补丁修复（`7cad8b7`）**。

## 一、交付面评价（合格）

1. **NoteRecord 树字段**：parent_id/kind/sort_order serde default 向后兼容 ✓（旧数据
   反序列化测试断言在）；
2. **环检测三态**（自挂/子树挂/祖先链）+ depth 上限 + `Error::CircularMove` 可读文案 ✓；
3. **delete_folder 改判子树批量软删**（无物理级联）——与 S1 回收站语义正确联动 ✓；
4. **restore 原位还原**：父存校验含 trash 判定（主链路）+ origin_path 逐级 title 解析 +
   兜底挂根（设计意图正确——但见 §二缺陷）；
5. **seal 形态统一 decode_record**：树面 NoteRecord 消费者 unseal 注入 ✓；
6. tauri 四命令注册 ✓（抽验 lib.rs）；禁改面零触碰（投影/搜索/事件字典/移动端）✓。

## 二、抓出的缺陷（复核核心价值）

### 缺陷 1（Test ❌ 根因）：`resolve_origin_parent` 漏 trash 判定

- **现象**：`restore_in_place_parent_alive_and_missing` 态2「父缺失挂根」稳定失败
  （两跑同点）——孤儿笔记还原后 parent_id 保留已删父 id，未挂根；
- **根因**：主链路 `restore_note` 父存活判定含「不在回收站」检查，但
  `resolve_origin_parent` 逐级找锚点 Folder 时只查 `note:` 物理键+kind+title/parent——
  **软删 Folder 物理键仍在 → trash 中的父被解析为活锚点**。同构判定缺失（`is_trashed`
  谓词现成未用，L860）；
- **修复**（7cad8b7）：resolve 循环内补 `is_trashed` 过滤（与主链路同构，注释注明）；
- **修复后**：dk02_s1_trash 11/11 全绿。

### 缺陷 2（Clippy ❌ 根因）：dk02_s1_trash.rs 两处 lint

`f2` unused + `t.title != ""` 空串比较——`-D warnings` 下 error。已修（`_f2`/
`is_empty()`，语义无损）。

## 三、CI 终态

| commit | Clippy | Test | Rustfmt | desktop | MSRV |
|---|---|---|---|---|---|
| 1b99eb6（Bravo 原交付） | ❌ | ❌ | ✅ | ✅ | ✅ |
| 7cad8b7（复核补丁） | 待查（本地矩阵已闭环） | | | | |

## 四、流程教训（重要——入任务书模板）

1. **Bravo 分批验证 ≠ CI workspace 口径**：CI 是 `--workspace --exclude aurora-desktop
   --all-targets -D warnings`——分批 `-p` 跑不覆盖（a）其它 crate 的 warning 升级、
   （b）workspace 联动。**交付报告「全绿」与 CI 事实不符 = 假绿**——本卡 Bravo 报告
   「boot5+11 全绿」但 bootstrap 集成测试在复核环境稳定失败；
2. **任务书补充验收项（即刻生效）**：交付前必须原样跑一遍 CI 口径命令（clippy+test），
   交付报告附原始 rc/摘要；
3. **同构判定走公共谓词**：trash 判定已在 `is_trashed` 公共谓词——手写内联判定必然漏
   （本卡主链路用谓词、resolve 侧手写即漏）——复核提出 crate 内规约：trash 语义一律经
   `is_trashed`，禁止内联重写。

## 五、卡关闭条件

- 7cad8b7 CI 五 job 绿（下会话首查）→ DK-02 S2 正式关闭；
- UI 面（树侧栏）按任务书为 Alpha 后续切片，不影响本卡关闭。

— Alpha 2026-09-29 午后
