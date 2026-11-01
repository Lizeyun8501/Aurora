# NOTEBOOK-OPT-01 — 笔记本优化任务卡（骨架）

> **状态**: 待派单（任务指令由用户单独下发给 Bravo——本卡只做域对账+开工锚点+验收纪律）
> **派单**: 用户 → Bravo（指令不经 Leo 转发）
> **基线**: f20e402（v27.2 年视图复刻——2026-10-31）
> **建卡**: Leo 2026-11-01 19:29

---

## 一、任务指令区（用户单独下发——此处留空）

```
2026-11-01 20:25 TA 授权「继续」（VIVO 原子笔记对标主线延续——
基于最优方式执行授权语义,Bravo 自主定向: 延续 589eaa8 编辑卡
的调研结论,落列表管理面手势对标——右滑收藏+滑动态收菜单语义）
```

---

## 二、域对账回执（Leo 开工前核查——Bravo 免重复摸底）

### 仓库状态
- **基线干净**: f20e402（v27.2）后无在途提交;git status 空——笔记本域无 assignee 冲突。
- **近期提交序列**: v27.2 年视图（f20e402）→ v27.1 五图对齐（e437a95）→ v27 CALENDAR-04（aae4bd1）——全 CALENDAR 域,未触碰笔记本域。

### 笔记本域代码面（现状锚点）
| 面 | 位置 | 规模/备注 |
|---|---|---|
| 前端主组件 | `apps/mobile/src/MobileApp.tsx` `NotesView`（1087 行起） | 单文件大段;含列表/搜索/收藏/加密面 |
| 列表渲染 | MobileApp.tsx 1164 / 1283 / 1776 三处 `.note-list` | 疑似主列表+搜索态+回收站态复用——优化时注意三处同步 |
| 笔记行结构 | 1044 `nt-row`+`nt-book`（含 depth 缩进树） | 笔记本嵌套树 |
| 后端 | `crates/aurora-core`（+entity/migration） | 笔记数据面/查询面 |
| UI 风格令牌 | `apps/mobile/index.html` `:root` 变量+`mobile.css` | 品牌蓝主题（**日历域 --cal-* 红为局部变量——勿混淆**） |

### 域边界（零冲突纪律）
- **独占**: `MobileApp.tsx` NotesView 段+`mobile.css` 笔记段+`crates/aurora-core` 笔记查询面。
- **共面敏感**: `MobileApp.tsx` 全局段（TabBar/FAB/捕获流）——若需动全局,commit 里显式标注。
- **禁触**: `CalendarView.tsx`/`repeatUtil.ts`/`--cal-*` 变量段（CALENDAR 域已闭环,回归卡锚定）。

---

## 三、验收纪律（Bravo 交付时必须携带）

1. **回执制**: 交付 commit message 含「验证:」段——列 Playwright 断言数+回归卡结果+tsc/clippy/fmt。
2. **回归锚**: 笔记本域动 `MobileApp.tsx` → 跑 CALENDAR 系回归卡（01/02/03/04+vivo）防共享面回归——`scripts/repro-cal0*.js`+`scripts/repro-vivo.js` 全绿才关账。
3. **出包**: 用户验收版才出 APK（默认不出,除非任务指令明确要求）。
4. **UI 类任务**: 截图+断言双通道（VLM 对拍可选——参考 CALENDAR-VIVO 快修模式）。

---

## 四、变更日志

| 日期 | 人 | 事件 |
|---|---|---|
| 2026-11-01 19:29 | Leo | 建卡（域对账+骨架）;任务指令留空待用户单独下发 Bravo |
| 2026-11-01 20:2x | Bravo | 开工（TA「继续」=最优方式授权）: 右滑收藏（VIVO 右滑设重要度对标）+swiped-fav 绿态+滑动态点 body 收菜单;复验 01/04 脚本 BASE 双路径（Leo 02/03 修过 01/04 遗漏）。验证: PW 7/7+CALENDAR 五锚回归全绿（01 6/6+02 7/7+03 6/6+04 9/9+vivo 7/7）+tsc 0 |
