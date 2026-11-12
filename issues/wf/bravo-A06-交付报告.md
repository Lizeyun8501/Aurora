# A-06 交付报告：移动端系统分享接收 + 小组件 / 快捷指令

> 执行: Bravo ｜ 工单: TA 2026-11-12 13:35 ｜ 终 commit: 9c99665 ｜ 后续 B-02 重构共存验证（Alpha 2872cd6/4c883e1 拆移——A-06 面全存活）

## 首节 · 开工令假设与代码实况披露

| # | 开工令假设 | 实况 | 处置 |
|---|---|---|---|
| 1 | 「经 Capacitor 插件送达 cmd_inbox_import（已有）」 | **本工程非 Capacitor 插件体系**（自建 AndroidBridge @JavascriptInterface 桥）;cmd_inbox_import=桌面 P2P SharePack 命令（配对直传语义——系统分享的原始文本/图片非 SharePack 形态） | 诚实等价: Android 原生 intent 队列 → AndroidBridge 桥 → JS 消费（既有五层链惯例）;落位目标语义对齐工单（当日日记默认+浮层可改） |
| 2 | iOS Share Extension / WidgetKit | **无 iOS 工程**（apps/mobile 仅 android/——移动端现仅 Android 落地） | iOS 面整卡登记（Xcode 工程面缺席——待 iOS 工程立起后按 App Group 共享容器方案落）;界面无 iOS 假象不产生 ✓ |
| 3 | 「落地页为当日每日笔记（见 A-12）」 | A-12（每日笔记）未落卡;既有「日记 YYYY-MM-DD」日记域（DiaryView/writeToday 惯例） | v1 落位=当日日记（既有惯例承载——title 命中即追加,缺则建）;A-12 真每日笔记落地后切目标语义为一等公民 |
| 4 | 任务提醒通知 | 既有提醒=日历事件域（ReminderScheduler 只扫 events） | 扩展扫描 calendarVm tasks 面（今日 due 未完成→到点通知带 taskId）——任务提醒原生落地 |

## 实现清单（Android 全链——javac 六真类编译实证 19 class）

### 分享接收（DoD ①）
- Manifest: ACTION_SEND 双 filter（text/plain+image/*）+singleTask（onNewIntent 运行态入口）
- MainActivity: 进程级 intent 队列（WebView 生命周期独立）——文本直取 EXTRA_TEXT（**UTF-8 String 零重编码——中文不乱码**）;图片 content URI 当刻流拷贝 files/shared/（读权限窗口内落盘）;三桥 consumePendingIntents（拉）/readSharedImage（base64 data URL 预览——WebView file:// 禁用绕行）/refreshWidget
- JS: ShareReceiveSheet 浮层（内容预览+**目标可改:今日日记（默认）/新建笔记**）;landShare 落位（日记追加引用块 `> 📥 [系统分享]`——title 惯例命中/缺建）

### 小组件（DoD ②）
- AuroraWidgetProvider: 今日待办数（core calendarVm **只读快照**——RemoteViews 纯原生渲染**零 WebView**）+快速捕获钮
- 刷新三通道: 系统周期 30min / **onResume 即推（回前台 <1s）**/ JS refreshWidget（任务态变更）
- 点击: 捕获钮→aurora://capture;主体→aurora://today（直达对应视图 ✓）

### 快捷指令
- aurora://capture（打开捕获 sheet+今日视图）/ aurora://today（今日视图）——BROWSABLE filter（系统浏览器/快捷指令 App 可直接触发）

### 通知双 action（DoD ③）
- **完成**: PendingIntent 拉起 app（aurora://task-done?taskId）→ JS 经 updateTasks→saveTasks（localStorage 权威+taskSync core 镜像→**投影=WritePath 语义,禁直改 DB ✓**）
- **延后 1 小时**: ReminderActionReceiver 纯闹钟重排（**零 DB 写**）+确认通知
- 任务提醒注册: ReminderScheduler 扫 calendarVm tasks（今日 due 未完成——requestCode=task:{id} 幂等覆盖）

## 验证矩阵
- javac 六真类全链编译（桩集 22 类——E25 验证法扩展: widget/notification/alarm 面）✅
- mobile tsc 零错 ✅;XML 双文件 minidom 解析 ✅
- CI（9c99665）: **desktop-check ✅ / Clippy ✅ / Rustfmt ✅ / MSRV ✅ / Wasm×3 ✅**（A-06 零 Rust 面）
- Test: 唯一 FAILED=dk02_s2_trash_flow_roundtrip——**五卡一致基线红**（A-01/02/03/05/06 同案分离）;A-06 面零失败
- B-02 共存: Alpha 重构后 landShare/ShareReceiveSheet/consumeA06Intents/桥三方法全存活+tsc 零错 ✅

## DoD 对照与诚实边界
- ✅ 分享文本+图片进入 Aurora（中文零重编码链路;**真机行为验证待装机**——PW 无系统分享通道,诚实披露）
- ✅ 小组件回前台 <1s（onResume 直推）;点击直达视图
- ✅ 通知「完成」经 WritePath 链回写（任务投影 done 链路=saveTasks→taskSync→core 投影;**端到端真机验证待装机**）
- ✅ iOS 不承诺后台保活（无 iOS 面——无假象;后续 iOS 卡如实规划）
- ⚠️ 登记项: iOS 整面（无工程）/图片真附件面（移动 FFI attachments=None——现为路径标记）/apps/mobile 无测试 runner（landShare 逻辑 tsc 验证,测试基建后续）/真机验证卡（微信/浏览器分享+小组件+通知 action 装机实证）

## 停手边界遵守
不做分享内容自动 AI 分类（只落日记/新建——用户显式选目标）✅ / 不做 iOS 后台常驻服务 ✅
