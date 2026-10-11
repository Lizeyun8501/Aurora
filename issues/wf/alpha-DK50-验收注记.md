# DK-50 Alpha 验收注记（关账+微修令）

**验收**: Alpha 2026-10-11 21:15 ｜**结论**: ✅ 带偏差关账（DoD2 单点未兑现 → DK-50.1 微修令内嵌） ｜**commit**: 2000ae0（单发，CI 零修复红）

## 终证渠道

- **CI 五检查全绿**（2000ae0）：Test / MSRV / desktop-check / Clippy / Rustfmt

## DoD 七条核对

| DoD | 结论 | 依据 |
|---|---|---|
| 1. badge 常驻+sheet 三入口 | ✅ | syncBadge prop 注入 AppBar（slot 设计不破布局）；gate 三态三通道+queue 徽标+冲突红点 |
| 2. 队列可浏览+冲突逐条决策+**联动即刷** | ⚠ | 面板/置灰防御/批量计数全✓；**onChanged 壳侧 `() => undefined` 空实现且无自注**——决策后 badge 最坏等 15s 轮询才刷（见偏差记录） |
| 3. visibilitychange 功耗裁决 | ✅+ | 三层防护：不可见跳请求+恢复即刷+interval 内查 visibilityState——超工单要求 |
| 4. 数据面零 Rust 变更 | ✅ | crates/ + android/** 零 diff |
| 5. 壳净增 ≤40 | ⚠+2 | 净增 42（44-2）；超标源于执行工单要求的 NoteEditor 侧单两入口+syncBadge slot 注释——实质合规，记偏差不追责 |
| 6. mock 回放路径 | ✅ | 无 AndroidBridge 走 platform 内存态；桥异常保持上次状态显离线缺省（防御好） |
| 7. CI 五门 | ✅ | 全绿 |

其余核验：mobile.css 34 个 `.sync-*` 前缀类✓；组件文件全在新建 components/✓；禁触面零触碰✓。

## 偏差记录（如实）

1. **onChanged 联动未接**（DoD2 明文要求，桌面 DK-48 已验证模式）：组件侧钩子完整（`onChanged?.()` 决策后调用），壳侧接线为 no-op **且无自注**。与 DK-49"差异披露"及本卡其余处的自注风格反差——更像漏接而非有意降级。有 15s 轮询兜底功能不破，故带偏差关账。
2. 壳净增 42>40：超标 2 行源于工单要求的侧单入口，实质合规。

## DK-50.1 微修令（同一卡补丁，随本注记派发）

**范围（单点）**：打通 onChanged→badge 即刷。
- 建议：SyncStatusBadge 加 `lastChangedAt?: number` prop（effect 内变化即 refresh），壳两面板 onChanged bump 一个 state——或等效 ref 句柄方案，Bravo 自选
- **解禁**：仅 SyncStatusBadge.tsx / MobileApp.tsx（净增红线本次放宽至 ≤55 总量）
- **DoD**：面板决策/批量操作后 badge 3s 内刷新；CI 五门全绿；其余零改动
- **规模**：XS

## 流水线注记

DK-48/49/50 三连收官"移动端同款"（数据面+UI 双端对齐桌面）。今天 8 小时三卡， Bravo 交付质量总体高——但本单 onChanged 无自注的漏接提示：**连发节奏下的装配末端易漏**，后续单发 Bravo 交付报告建议加"挂接清单自查"一节。
