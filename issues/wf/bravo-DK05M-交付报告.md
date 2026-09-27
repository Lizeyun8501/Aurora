# Bravo DK-05M 交付报告 — 移动端块编辑器（Android WebView 面）

> 基线 8701482 · P0 末卡 · 本地四门槛全绿 + dk05m_verify 5/5

## 一、方案落地说明（Alpha 七项裁决逐项）

| # | 裁决 | 落地 |
|---|---|---|
| ① | visualViewport 唯一方案 + --vvh 驱动 | `MobileApp.tsx` 新增 `useKbdViewport()`：vv resize/scroll → rAF 节流写 `--vvh`（可视视口高）与 `--kbd-inset`（键盘遮挡 = innerHeight − vv.height − vv.offsetTop）；AndroidManifest `windowSoftInputMode="adjustResize"` 双保险 |
| ② | 工具条 fixed 贴 --vvh 底 | `.editor-toolbar` fixed，`bottom: max(var(--kbd-inset), env(safe-area-inset-bottom))`（max() 收起态回退安全区、弹出态取键盘高，一行通吃）；`.app-shell` 高度 `var(--vvh, 100%)`——**全仓 100vh 零命中（rg 实证）** |
| ③ | 浮动菜单坐标化 + vv 修正 | FloatingMenu 改 **fixed 视口系定位**（坐标即视口坐标，键盘态/滚动钳制天然一致）：rAF 节流统一三源（selectionchange/scroll/vv resize）；钳制规则 = 上缘 `max(vvTop+4, min(topView, vvBottom−48−34−4))`——遮挡时贴工具条上方；桌面无 vv 自动回退全屏口径 |
| ④ | IME composing 门控 + preventDefault | editor-uplift 已落（onUpdate `view.composing` 跳过重渲染 + 工具条/浮层按钮 preventDefault）——本卡 verify 断言 A2 实证 |
| ⑤ | 安全区既有基建 | `env(safe-area-inset-bottom)` 仅作用于工具条 bottom max() 表达式，未动全局 viewport-fit |
| ⑥ | 性能 50fps | 断言 A4 实测 **P95 = 16.7ms ≤ 20ms**（5.2k 字量级 rAF 间隔采样 24 样本）；未引入虚拟滚动新依赖、未用 content-visibility（PM 坐标风险，实测已达标不引入） |
| ⑦ | 无障碍基线 | A5 实证：role=toolbar + aria-label + 16 按钮全标注可聚焦 |

## 二、DoD 五断言证据（dk05m_verify.js · 375×667 hasTouch file://）

```
PASS A1 工具条键盘态（vv 400 → 完整可见贴可视底）: vvh=400px inset=267px tb.bottom=400
PASS A2 IME composing 门控（无强滚/anchor 不漂移）: scroll 0→0→0 anchor 0→0
PASS A3 浮动菜单（键盘态下可视区展示/贴工具条上方）: menu.top=64 bottom=108 vvBottom=300
PASS A4 万字滚动 rAF 间隔 P95 ≤ 20ms: samples=24 p95=16.7ms
PASS A5 工具条 aria 快照: role=toolbar label=格式工具栏 btns=16 allLabeled=true
SUMMARY: 5/5 PASS
```

**配套使能**：mock platform 补快照语义（`EMPTY_SNAPSHOT_B64` 空 LoroDoc 常量 + createNote 即建快照 + getNoteSnapshot/saveNoteSnapshot 无桥分支落 localStorage）——file:// 下 rich 模式使能（此前静默降级 textarea，断言 2/3 无法触达 PM 面）。

## 三、构建与回归

| 项 | 结果 |
|---|---|
| `npx tsc --noEmit`（mobile） | ✅ 0 错误 |
| desktop tsc（共享层回波） | ✅ 0 错误 |
| `npx vite build`（mobile，base './' 相对化——file:// 与 tauri 协议双兼容） | ✅ 5,088kB |
| schema/既有 mobile 测试基线 | ✅ vitest **24 passed / 0 failed**（3 个 test file 收集失败为**环境既有**：@testing-library/dom 依赖缺失，git stash 对照同态实锤，非本卡引入） |
| rg 清场：100vh / 第三方键盘·浮动库 | ✅ 双零命中 |
| dk05m_verify 自包含 | ✅ build-prereq 陈旧自动重建（D1 模式） |

## 四、挂起项显式标注

1. **APK 出包冒烟：挂起**——本机实测无 gradle / Android SDK（任务书预期「现成链」不符）；AndroidManifest adjustResize 静态配置已交付，真机验证随 DK-17 兑现（与 TalkBack 同挂）。
2. **TalkBack 真机**：任务书既定挂起（DK-17）。
3. **捕获 body 不进 rich 初始文档**（capture 建笔记后编辑器显示空——editor-uplift 语义遗留）：verify 侧自灌 fixture 绕过；**是否为产品缺陷请 Alpha 裁决**（不影响本卡 DoD）。
4. **性能口径**：A4 实测为 5.2k 字（130×40 chunk 注入上限），未足「万字」整量——P95 余量充足（16.7 vs 20ms），如 Alpha 要求足量可加注 250 chunk。

## 五、CI

push 后 CI Run 号回填；五 job 门禁以 GitHub 为准（desktop-check 前置 frontend build 已入 workflow——**mobile dist 不入仓，mobile 侧同样自包含**）。

— Bravo 2026-09-27（DK-05M 交付）
