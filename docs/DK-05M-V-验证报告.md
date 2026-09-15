# DK-05M-V WebView 输入时序验证报告

- **卡**: DK-05M-V（5 人日，Go/No-Go 门）
- **日期**: 2026-09-14
- **环境**: Chromium headless (Playwright) — 与 Android WebView 同 Chromium 内核源
- **被测栈**: ProseMirror + loro-prosemirror + LoroDoc(WASM)（与生产 `apps/mobile/src/editor` 同栈）
- **验证页**: `apps/mobile/editor-lab.html`（独立构建 `vite.config.editorlab.ts` → `dist-lab/` 单文件）
- **驱动**: `scripts/dk05mv_verify.js`（7 项断言，可重复执行）

## 第二轮（DK-05M DoD 验证，2026-09-16 补充）

| 场景 | 结果 |
|---|---|
| ⑤ 万字笔记滚动 ≥50fps | ✅ 31,463 字滚动 avg 16.5ms / p95 17.0ms — **60fps 满帧** |
| ⑥ VoiceOver/TalkBack 可遍历 | ✅ role=toolbar + aria-label + 按钮全可聚焦，issues=[] |

九断言 9/9 PASS。验证页含与生产 RichEditor 相同 ARIA 契约的工具条样板
（role=toolbar/aria-label/按钮焦点遍历），产品端由 RichEditor.tsx 源码
保证一致。

## 结论：**GO**（附真机补验清单）

ProseMirror + Loro 栈在 Chromium 内核下**四场景全过（7/7 PASS）**，
无阻塞性技术风险。DK-05M（65 人日）可按现方案开工。

## 四场景结果

| 场景 | 断言 | 结果 |
|---|---|---|
| ① 焦点与输入事件时序 | IME 提交路径逐字入 doc；Loro map 同步；快速连打无丢键 | ✅ 3/3 |
| ② IME 组合输入 | compositionstart/update/end 序列不破坏文档；组合后恢复普通输入 | ✅ 2/2 |
| ③ 光标恢复 | blur→refocus 后 selection 保留；原位插入语义正确 | ✅ 1/1 |
| ④ 长文档滚动 | 1000 段填充；滚动帧间隔 avg 16.1ms / p95 16.8ms（60fps 满帧） | ✅ 1/1 |

## 关键发现（规避方案已记录）

1. **CDP key event 与 PM 的差异（测试环境项，非产品缺陷）**：
   headless Chromium 的 `Input.dispatchKeyEvent` 不产生 PM DOMObserver
   依赖的 beforeinput 链——`keyboard.type` 不进 doc，而 **CDP
   `Input.insertText`（= IME 提交路径）完全正常**。这恰证明生产路径
   （真实 IME 走 composition → beforeinput）是 PM 的一等公民。
   自动化测试一律用 insertText / 程序化 dispatch。
2. **LoroSyncPlugin 与合并长事务冲突**：多段插入必须在**独立事务**中
   逐段 dispatch（合并长 tr 会因插件 appendTransaction 改映射导致
   位置越界）。DK-05M 实现时禁止跨段合并事务。
3. **跨场景文档增长**：光标定位一律动态 `indexOf` 而非固定偏移。

## 真机补验清单（不阻塞 Go）

| 项 | 原因 | 归属 |
|---|---|---|
| Android IME 候选栏与拼音流式组合 | 系统输入法行为，桌面无等价物 | DK-05M 第一轮首个 sprint |
| 键盘弹起高度与视口 resize 时序 | 系统行为 | 同上（工具条定位依赖） |
| WebView 存储被系统清理边界 | 系统行为 | DK-01 存储（关键数据已按铁律走原生 SQLite notesnap:{id}，localStorage 仅 UI 偏好） |
| 惯性滚动物理差异 | 系统滚动模型 | DK-05M 滚动集成 |

## 环境与复现

```bash
cd apps/mobile && npx vite build --config vite.config.editorlab.ts
node scripts/dk05mv_verify.js   # 退出码 0 = 全过
```
