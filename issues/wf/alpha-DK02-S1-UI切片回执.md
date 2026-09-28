# Alpha DK-02 S1 UI 切片回执 + Bravo core 面复核确认

## 一、Bravo DK-02 S1 core 面（f175692）— 快速复核确认

（UI 切片依赖其接口，联调前格式级复核；深度复核随卡关闭时一并做）

- **四项任务书裁决全遵守**：trash 标记键 {deleted_at_ms, title 快照} / delete_note 签名不变物理键保留 /
  附件级联挪 purge / NoteDeleted 照发；
- **restore 事件选型裁决落地**：NoteCreated 重放（投影禁改零触碰 + 字典冻结面零追加）——比新增
  NoteRestored 更保守，投影消费方零改动；open_note_content 锁定态 fail-closed 顺手加固；
- **purge_note 仅 trash 中生效**（防误删未删笔记）+ 附件级联回归；
- 测试面 dk02_s1_trash.rs 352 行 + 437 全绿 + clippy 0 + fmt 净；
- **挂起项诚实化**：搜索投影「verify 索回缺口」（restore 后重新索引需 source 回调加 trash 过滤——
  S2）+ source 回调独立排障 + 跨端投影消费（mobile）。

## 二、UI 切片（3479da7，Alpha）

1. **TrashView.tsx**（自包含组件，ImportWizard 先例形态）：
   - 对真实接口联调形态：cmd_list_trashed / cmd_restore_note / cmd_purge_note（f175692 签名）；
   - **彻底删除 confirm 门**（不可逆操作防误触）+ busy 互斥（单飞行请求）+ 错误 alert 态；
   - 四态渲染：mock 提示（invoke=null）/ 加载 / 错误 / 列表（title + 删除时间 + 双按钮）；
   - **aria 完整**（DK-05M 纪律）：list/listitem role、恢复/删除按钮 aria-label 含笔记标题、
     状态区 role=status/alert；
2. **DesktopShell 接线四处**：MainView + 'trash' / 导航第四 tab（恒显，mock 显示提示态）/
   label 映射 / 内容分支（canvas 后）；
3. **验证**：tsc + vite build 双绿；**端到端联调挂起**（本机无 tauri 运行环境——core 面单测已
   证行为，UI 运行时冒烟随 APK/桌面构建联调，S2）。

## 三、DK-02 卡进度与遗留

- S1 core+UI 双面就位（待端到端联调冒烟）；
- **S2 已知项**（Bravo 挂起 + 派发预告）：搜索投影 source 回调 trash 过滤（verify 索回缺口——
  **重要**：不修则恢复后搜索仍搜不到）、source 回调排障、跨端投影消费、30 天自动 purge、
  回收站批量操作；
- 卡保持 open 至 S1 端到端联调 + S2 索回缺口修复。

— Alpha 2026-09-28

---

## 补记：深度验收终裁（2026-09-28 17:50）

- **CI Test 红灯真相**（Bravo bb25272/57c8f95 自查自修）：f175692 首轮失败 = **mobile 列表软删泄漏**
  （软删后 note: 键保留，mobile list_notes 不过滤 trash——任务书预警点命中）+ **import 清场隐藏依赖**
  （delete_note 物理语义被 import 失败清理依赖——软删后产生半截笔记残留）。修复轮 57c8f95：
  mobile-ffi list_notes 过滤 trash + is_trashed 谓词 + **discard_note 原语**（delete+purge 连招）
  替换 import 3 处——**修复轮五 job 全绿**；
- **Alpha 本地独立验证**：aurora-core 393+5+15 / bootstrap 5+2 全绿（含 dk02_s1_trash 全行为）——
  本地未复现 CI 红（复现面未含 mobile-ffi/import——**教训：验收复现面必须覆盖改动波及面**）；
- **Alpha 本地全量 workspace 编译撞盘（30G 盘被多轮全量编译撑爆，linker Bus error）**——已清；
  全量验证以后用 `--exclude aurora-desktop` 分批 + 例行清盘；
- **S2 request 批复**：搜索投影 trash 过滤 Bravo 即刻执行（0.5 人日）；加密笔记 rebuild 出索引
  缺口立卡归 DK-20；
- **DK-02 S1 关闭**（core+UI 双面 + CI 修复轮五绿 + S2 前置 request 已批）。卡 open 余 S2。

— Alpha 终裁
