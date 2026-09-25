# DK-05 S0 桌面输入时序复验报告

> 日期: 2026-09-25 · 执行: Alpha · 任务书: `issues/wf/alpha-DK05-desktop-editor.md` S0
> 环境: Playwright(Chromium 1280×800) + `apps/desktop/dist-lab/editor-lab.html`
> 被测: 共享层编辑器实体（`@aurora/ui-components`，editor-uplift 00074ba 上移产物）
> + loro-prosemirror 0.4.4 + loro-crdt 1.14.1（内存 LoroDoc，无持久化桥）

## 1. 结论：**无阻塞性缺陷，S1 可开工**

**10/10 PASS**（`scripts/dk05_desktop_verify.js`，五场景）：

| # | 场景 | 断言 | 结果 |
|---|---|---|---|
| ①a | 焦点可达 + 逐键输入落 doc | activeElement=ProseMirror + 文本入 doc | PASS |
| ①b | Loro 同步插件生效 | loro doc 经 LoroSyncPlugin 可访问 | PASS |
| ②a | 快速连打（60 字符 delay:0） | 无丢键无乱序（endsWith 断言） | PASS |
| ②b | doc 尺寸单调一致 | content.size > 0 | PASS |
| ③a | IME composition 序列 | start/update×3/end + 提交落文 | PASS |
| ③b | 组合后恢复普通输入 | COMPOSED 落 doc | PASS |
| ④a | **滚轮滚动中输入不丢行**（桌面差异项） | 400 段长文档 + wheel×6 并发窗口 insertText | PASS |
| ④b | **滚轮并发插入落 doc** | 并发窗口文本入 doc | PASS |
| ⑤a | 长文档光标恢复 | scrollIntoView 后 selection 保持 | PASS |
| ⑤b | 长文档滚动状态可读 | scrollHeight/scrollTop 可采样 | PASS |

## 2. 附带验证（S0 顺带收益）

- **共享层实体桌面可用**：lab 页经 vite alias 直接 import `@aurora/ui-components`
  （createAuroraEditor + auroraSchema），桌面构建栈解析、运行、类型全通——
  S1 接入无实体层风险。
- **EditorPlatformBridge 未涉**：S0 用内存 LoroDoc（onSave 恒 true）；
  持久化桥链路归 S2 落库验证。

## 3. 风险清单（非阻塞，S1/S2 随带关注）

1. **真 WebKit/WebView2 未验**：本复验为 Chromium 源（与 mobile 同理由）。
   Tauri 桌面三引擎（macOS WKWebView / Windows WebView2 / Linux WebKitGTK）
   的 IME/滚轮实现差异需打包后冒烟（S1 只读接入后随真机窗口复验一次即可）。
2. **composition 候选流为合成事件**：真实 IME 候选窗行为（如中文输入法
   逐键上屏）与合成序列存在差异；mobile 版同款局限（DK-05M-V 报告已声明），
   桌面真机冒烟同 S1 随带。
3. **400 段 ≈ 8K 字滚动 60fps 未采样**：本卡只验时序正确性；性能采样
   （万字档）建议 S2 可编辑态接入后一并做（mobile 版万字基线已有）。

## 4. 产物

- 验证页: `apps/desktop/editor-lab.html` + `apps/desktop/vite.config.editorlab.ts`
  （构建: `npx vite build --config vite.config.editorlab.ts` → `dist-lab/`）
- 驱动脚本: `scripts/dk05_desktop_verify.js`（exit 0 = 全过）

— Alpha 2026-09-25
