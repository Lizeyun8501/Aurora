# Aurora Note V26 · 待做任务卡（41 卡全文存档 · 2026-11-24 用户重发版）

> 本文件为派发依据存档——单卡正文含「现状证据→任务步骤→验收→停手边界」。
> 通用前缀（铁律十条+输出要求）见 §0。派发纪律：一次一卡;共用文件串行。
> 批次一（O 系 9 卡）已于 2026-11-24 全闭环——批次二/三卡文自本档派发。

**基线**：main@0796f9a（2026-11-20，889 commits）· 归档实测 Rust 102,986 行 / TS 34,198 行 / 1,654 测试函数
**总规模**：41 卡 · 三批次（一 9 卡/8-12 人日 · 二 10 卡/50-70 · 三 22 卡/70-100）

## 0. 通用前缀

你是一名负责 Aurora Note 仓库的开发 agent。只处理本卡范围，不扩大。

【基线】main@0796f9a。Rust 1.91.0 + wasm32-wasip2；workspace 15 members；
前端 apps/desktop（Tauri）、apps/mobile（Capacitor v6 + WebView）、
shared/ui-components（84 文件，被 27 个文件真实 import）。

【铁律 · 违反即视为交付失败】
1. 本地优先：无网络全功能可用；禁 CDN、禁遥测、禁云端账号依赖。
2. 写入纪律：A 类（笔记/块/树/附件）必须经 write_path/，禁止 core.kv_store 直写；
   B 类配置 KV 须登记豁免清单（O-02）。
3. 禁止伪成功：未实现分支不得返回 Ok(vec![]) / Ok(json!({"ok":true})) / 空集合 / 伪文本；
   必须返回 NotImplemented 或直接删除该分支。
4. AI 永不静默写入：两段式 aiLiquify 提案 → 用户勾选 → aiCommit 才落库。
5. 投影可重建：派生失败不阻断主流程，启动可依水位线重放。
6. 迁移三段式：新增 → 双写 → 切读 → 观察一版 → 删旧；禁止直接改既有表后删列。
7. 设计令牌：颜色/动效只用 var(--*) 与 tokens.ts；禁硬编码 hex/rgba/自定义时长。
   ADR-006 暗色默认 + 青色主色，旧靛蓝 #4F46E5 / 侧栏 #111827 已废止。
8. 无障碍：aria/role/键盘可达/语义标签由 VM 携带；状态不得仅靠颜色;
   须支持 prefers-reduced-motion。
9. 新增事件必须同步 event.rs / events.ts / EVENT_DICT 并走 ADR-002。
10. 每次改动必须让 CI 变绿或保持绿。

【输出要求】改动文件清单+验证命令+验收逐条结论;阻塞如实报告;不越停手边界;
关账给 commit 锚点。

## 2. 总览表（批次二/三余 32 卡）

| 卡号 | 标题 | 批次 | 优先级 | 人日 | 依赖 | 主要文件 |
|---|---|---|---|---|---|---|
| F-01 | 公开发布四重防护+协作 v2 | 二 | P1 | 12-18 | O-01 | share_commands.rs |
| F-02 | 移动端附件预览接线 | 二 | P1 | 8-12 | O-01 | MobileApp/NoteEditor/AttachmentPreview |
| F-03 | 任务依赖 UI | 二 | P1 | 5 | O-01 | TodoView/TasksView |
| F-04+U-06 | 右滑完成/左滑计划+手势工具抽 shared | 二 | P1 | 5 | O-01 同派 | TasksView/TodayView/useSwipeGesture |
| U-02 | 硬编码 hex 清零+CI 门禁 | 二 | **P0** | 6-8 | O-01 串行 U-03/08/11/13 | mobile.css/themeVm/tokens |
| U-04 | 无障碍三缺口 | 二 | P1 | 6 | O-01 | a11y.css/eslint/walkthrough |
| U-05 | 导航注册表+IA 图 | 二 | P1 | 3 | O-01 | navigation.ts/MobileApp ⚠F-02 |
| F-07 | 移动日历切读 Rust VM | 二 | P2 | 3 | O-01 | CalendarView/FFI 桥 |
| F-11 | 插件市场列表页 | 二 | P2 | 8 | O-01 | MarketplaceView |
| U-01 | DesktopShell 5420 行拆分 | 三 | **P0** | 15-20 | U-05 | DesktopShell ⚠独占 |
| U-03 | V26.1 界面规范修订 | 三 | **P0** | 8 | U-02 同派 F-14 | V26 规格六部/ADR-006 |
| F-14 | 界面设计资产入仓 | 三 | P2 | 4 | U-03 同派 | docs/design/screens/ |
| U-11 | 深色对比度实测 | 三 | P2 | 2 | U-02 | check-contrast.mjs |
| F-08 | 混合检索 RRF+语义徽章 | 三 | P2 | 10 | O-01 | hybrid_search/SearchView |
| F-10 | Agent 工具时间轴/Kill-Switch | 三 | P2 | 10 | O-01 | AgentView/LiquifyReview |
| F-05 | CRDT 画布协同+移动只读 | 三 | P2 | 12 | O-01/O-06 | CanvasView/crdt.rs |
| F-06 | 24h 像素轴周视图 | 三 | P2 | 2 | O-01 | CalendarPanel |
| F-09 | N 跳图遍历 | 三 | P2 | 6 | F-08 | GraphPanel/bidi_link |
| F-12 | i18n/RTL 契约 | 三 | P2 | 6 | O-01 | shared/types |
| F-13 | 性能基准对测 RV-01 | 三 | P2 | 4 | O-01 | bench.yml |
| O-07 | 测试覆盖补强 | 三 | P1 | 10 | O-01 | migration/mobile-ffi/desktop |
| O-10 | 依赖与平台版本 | 三 | P2 | 4 | O-01 | package.json/Tauri |
| O-11 | 备份 RPO/RTO 实证 | 三 | P2 | 3 | O-01 | backup-drill.yml |
| O-13 | 插件隔离文档统一 | 三 | P2 | 1 | O-01 | README ⚠/security-model |
| U-07 | FAB 状态机 | 三 | P2 | 2 | U-02 | mobile.css/MobileApp ⚠ |
| U-08 | CSS 拆分 | 三 | P2 | 4 | U-02 同派 U-13 | mobile.css ⚠ |
| U-13 | 动效令牌三档 | 三 | P3 | 1 | U-02 同派 U-08 | tokens/a11y.css |
| U-09 | 四态变体矩阵+冲突屏 | 三 | P2 | 6 | U-02 | shared/views/states.md |
| U-10 | 复用率度量 | 三 | P2 | 2 | U-01 | measure-ui-reuse.mjs |
| U-12 | 平板/折叠屏回归 | 三 | P2 | 2 | U-05 | MobileApp ⚠/E2E |
| U-14 | 桌面三栏栅格 | 三 | P3 | 2 | U-01 | DesktopShell ⚠ |

单卡完整正文（现状证据/任务/验收 DoD/停手边界）以用户 2026-11-24 20:49 重发原文
为准——会话级传递不落盘;如需单卡派发请用户重发对应卡段。
本档锁总览+通用前缀+铁律,保证派发口径不漂移。
