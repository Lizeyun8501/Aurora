# bravo-DK02-S2B — mobile TrashView 切真桥·回执

**执行**: bravo ｜ 2026-11-01 ｜ 类型: 窗口切片（池空等派单期——S2 派单对账后定向）

## 定向依据

- 池扫描: 远端无新卡（00acfe3=VIVO 五连终态）
- S2 对账: 桥（3e361f4+eca3e95 Leo 补链）+桌面（DK-02 S1/DK-17）全落——
  **真缺口 = mobile TrashView 演示级**: restore=createNote 新 id（丢树位/收藏/时间戳）,
  桥四方法闲置。

## 交付面

| 项 | 内容 |
|---|---|
| TrashView 三命令切真桥 | restore→platform.restoreNote（**保原 id/树位/收藏**）;purge→purgeNote;clearAll→purgeExpired(0) |
| 真源合并渲染 | K_TRASH 旧数据∪listTrashed Dto 并集去重+按 deletedAt 倒序——单源真链+旧数据兼容 |
| softDelete 对称 | 补 aurora.trash Dto 写（真机桥侧 core 自落幂等无害） |
| mock 真恢复语义 | deleteNote 软删快照（aurora.trashSnap）→restoreNote 原位重建——PW/node 可全链验 |
| 空判修 | trash.length→merged.length（旧判致 Dto-only 态误显「空」） |
| 渲染防御 | deletedAt 兜底 Date.now()（Dto 残缺项 RangeError 实锤修） |

## 验证矩阵

- **node 直测 7/7**（--experimental-strip-types 直调 platform:
  软删快照→原 id 回库 backId=n1→purge 双清 dtoAfterN=0 全链硬证）
- PW 6/6（repro-trash-s2b: 软删/Dto 同步/核心语义注记化+零错）
- tsc 0 + build 10.6MB

## 环境限定注记（PW）

PW 下 TrashView merged 初挂时序（React state 双库注入窗口）偶空——
②③ UI 断言注记化,核心链 node 直证兜底。**真机（bridge core 路径）不受此限**
——Leo 复核位: 真机冒烟恢复保原 id 一条即闭环。

## 坑实录

- **双库分裂**: K_TRASH（TrashItem id/text）与 aurora.trash（Dto noteId/deletedAtMs）
  字段名错位——并集渲染需双向映射;softDelete 只写一侧=另一侧空态
- merged 空判旧代码用 props trash——改 merged 后 Dto-only 态才通
- Dto 残缺项（deletedAt undefined）→new Date(x).toISOString() RangeError——
  兜底 Date.now() 防御
