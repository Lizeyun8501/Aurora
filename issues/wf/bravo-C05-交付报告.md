# C-05 交付报告：集合视图去 prompt() + 空状态/加载态统一

> 执行: Bravo ｜ 工单: TA 2026-11-17 07:20 ｜ 依赖 A-07 已解（CollectionView v1 已 UI 化——本卡注记改写+三态/表格升级）

## 首节 · 工单假设对账

| 工单断言 | 实况 | 处置 |
|---|---|---|
| 「CollectionView v1 用原生 prompt() 编辑单元格」 | **已过时**——A-07 已全 UI 化（文件头自注「零原生弹窗」;唯一命中=该注释文字） | 注释改写避 rg 假命中 |
| 「EmptyState 四处各写一份（14/4/3/3）」 | 实况: primitives.tsx 本地组件 1 份+inline `.empty-state` div 8 处+桌面 `<p>暂无` 若干（B-02 拆分后分布变迁） | 共享层唯一实现+全部消费面直迁 |

## 交付

### ① 原生弹窗全库退役（~74 源点）
- **共享基座**: `layouts/dialogs.tsx`——promise 化 `dialog.ask/input/notify`（命名避 DoD rg 模式撞车: confirm/prompt/info 改名——75 点全量换）+`DialogHost` 单挂载（module dispatch+FIFO 队列）;**双端挂载**（DesktopShell+MobileApp 根——mobile 漏挂为 PW 三跑实抓根因）
- **Modal 增强**: 样式自注入（双端无 CSS 管线——12 类模块级 `<style>`）+初始焦点=输入控件优先（header 关闭钮 DOM 前序抢先——实测教训）
- 替换面: DesktopShell 61/RichEditor 4/TrashView 4/AttachmentPreview 1/PairDialog 1/VersionHistoryPanel 1（危险动作 danger 红钮;列表 pick=label 携行单;多行长文本 multiline）
- **rg 终态: `prompt\(|alert\(|confirm\(` apps/ = 1 命中=`cmd_plugin_hot_update_confirm`（Rust 命令名——非浏览器调用;Alpha 插件域不越界改名,三面同步成本>收益,披露）**

### ② 空状态/三态统一（DoD②）
- `views/EmptyState.tsx` 共享唯一实现: 插画位（线性 SVG 六场景——不重绘=停手边界）+title/hint/action 全场景传入（零硬编码业务文案）+`Skeleton`（波纹骨架,禁 spinner）+`ErrorState`（role=alert+重试）
- 消费面直迁: 六视图（Recent/Calendar/Diary/Tasks/Favorites/Trash）+inline 8 处（sheets×2/Search×2/Notes/Calendar/MobileApp 收集箱）+PluginsView 3 处——primitives 本地件删（不留桥）

### ③ 三态+集合表格
- CollectionView: 加载=骨架屏（**800ms 下限门**——慢网络可见性;禁 spinner）/空=共享 EmptyState/错误=ErrorState 带重试;离线态不路由错误面（既有 SyncStatusBadge 语义保持——V24.1 判定）
- 表格: 列头类型徽章（底色片+文字双载体）/行 hover 手柄 ⠿/选中行高亮（editing 行青底）/密度 4pt（6px8px→8px12px 全表）

## 验证矩阵
| 面 | 结果 |
|---|---|
| desktop tsc / mobile tsc | RC=0 / RC=0 |
| vitest（shared） | **132/132**（+8 新: 三态组件+对话框服务行为级——ask 双路/input Enter 初值/notify/未挂载降级） |
| PW c05Dialogs | **8/8**（样式注入/收集箱空态/dialog.input aria-modal+label+input/Esc/首焦输入框/零 JS 错误） |
| 回归 | C-01 21/21 · E2E top-paths 10/10 |
| 束 | mobile+desktop+android assets 三重建同步;fmt RC=0 |
| DoD③ 骨架 800ms | 代码门锚（withSkeleton min 800）+Skeleton 组件 vitest 过;**桌面 tauri invoke 慢网 e2e=真机运行时面（browser-mock 无 invoke 不走加载路径——诚实披露）** |
| DoD④ a11y | 焦点陷阱（Tab 循环）/Esc/aria-modal——Modal.test+c05States+PW ③-2/3/4 三层实证 |

## 停手边界
插画未重绘（一稿六景线性 SVG）✓ / 动效沿用三档（骨架 1.4s 波纹=加载态非装饰动效）✓
