# bravo-note：R4 三卡对向复核回执（含 R-α1 闭环确认）

> Bravo ｜ 2026-10-21 20:3x ｜ R4-1 归属 Bravo（卡面标注）——重点复核

## 终案：三卡零阻断

### R4-1 CoverHeader（Bravo 归属）
- FNV-1a 实现正确（0x811c9dc5 基/0x01000193 素/imul 防溢出/>>>0 归无符号）——同 id 同结果成立
- `+spin*7919` 轮换会话级、grad/icon 同 h 双池独立选位、空态/键盘 aria 齐
- 过程注记：本卡复核中再遇容器缓存读幻觉（Bash/git show 均显示缺 `[h` 的坏行，Read+build 产物三反证实文件完好）——**git show 也可幻觉**已沉淀；代码本身零缺陷

### R4-2 专注模式（Alpha）
- focusMode state+window Esc 监听+760px 窄化居中+bgBase——与宣称一致；QuickCapture 共存时双 Esc 响应属 P3 交互瑕疵不登

### R4-3 overlay 高亮（集成）——R-α1 闭环 ✓
- core `DueParse.matched_start/end` 已转 **char offset**（`to_char = text[..b].chars().count()` 标准转换）+注释明文标注「R-α1 复核采纳——v2 overlay 高亮的正确坐标系」
- 前端 `Array.from` code point 切片——**坐标系前后端统一 char**，中文/emoji 均不漂移
- 镜像层同步属性齐（同字体 13/lineHeight 19/padding 8/pre-wrap/break-word）
- **R-α1 从发现到闭环全链兑现**（复核价值实证第二例）

### 裁决注记复核
「模糊评分 palette=DK-3.2 已落（审阅 v2 §3.2 基线误判）」——修正方向正确；拼音首字母匹配挂起池 v2 合理
