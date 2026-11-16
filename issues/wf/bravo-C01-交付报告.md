# C-01 交付报告：移动端导航与 V25 规范对齐（ADR-007 裁决+落地）

> 执行: Bravo ｜ 工单: TA 2026-11-16 09:19 ｜ 依赖 B-02 已解（导航面为 MobileApp 局部——未触巨石重构）

## 首节 · 裁决（ADR-007 落盘——单一事实源 `docs/adr/ADR-007-mobile-navigation.md`）

**代码为准局部修订**（工单推荐案采纳）:
- 日历保留第 5 Tab（62KB 实现+CALENDAR-VIVO 12 卡迭代+用户在用——不为对齐删掉）
- Inbox 主入口补齐（GTD 收集环节真空=真缺口——捕获原直跳编辑器,无收集缓冲）
- 与 V25 原规范两处偏差披露入 ADR: 搜索=AppBar+⌘K 非Tab（2026-10-28 裁决承继）/日历进 Tab
- v26_issue清单 两项关账（导航重构/设置移出主栏——不再两套说法）
- V25 大文档已退役（1004db7）——规范侧修订落点=ADR-007+清单（README 文档树体系）

## 落地（commit 内全码）

| 面 | 实现 |
|---|---|
| 五 Tab | 今日/笔记/**收集箱**/任务/日历（TAB_OF_VIEW/VIEW_TITLES/TabBar items） |
| Inbox 语义 | 收集:捕获面板「存入收集箱」按钮（存后跳收集箱——去处可视）;处理:转笔记/转任务（今日,E17 语义）/删除/清空;localStorage `aurora.inbox`（E21 前端方案惯例）;直出通道（生成笔记/任务/日程）保留=快慢双路径 |
| 设置二级化 | AppBar 撤 Agent 位（E16 退役）→笔记页齿轮（onSettings 槽,仅 notes 视图渲染）+抽屉头像行双入口;Agent→抽屉「更多」组 |
| ⌘K | meta/ctrl+K→搜索视图（keydown 全局） |
| FAB | 右下（现状已合规——无中央凸起,披露）;键盘避让=visualViewport 收缩<80% 隐藏（fab-kb-hidden+aria-hidden） |
| a11y | nav role=tablist+aria-label=主导航;tab role=tab+aria-selected+aria-label;图标文字双载体;56px≥48;安全区 env() 既有规则（断言锚样式表） |

## 验证矩阵
- **C-01 断言 21/21**（scripts/tests/c01Nav.mjs——tablist/五标签序/双载体/选中态翻转/收集箱空态/存入条目/转任务清空/今日可见任务/TabBar 无设置/笔记页齿轮独占/Agent 无 AppBar+在抽屉/Ctrl+K/FAB 右半区/安全区规则/键盘避让 display=none）
- **E2E 回归 10/10**（旧标签选择器 4 处同步:知识库→笔记）;vitest 124/124;tsc 零错;fmt RC=0;dist 重建
- DoD④（VoiceOver/TalkBack 真机遍历）: role/aria-selected/双载体齐备=可遍历结构性前提;真机读屏验证留真机面（PW 无读屏——诚实披露）

## 停手边界
不做导航项自定义排序（v2）✓ / 日历视图未删（升级为 Tab）✓ / 不改 WIT（不涉）
