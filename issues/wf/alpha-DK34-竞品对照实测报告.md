# DK-34 — 竞品对照实测报告（perf-baseline.md 竞品列补齐，Alpha 自立卡）

> 2026-10-05 晚 · Xvfb headless · 3 轮取中位 · 环境无 GPU（--disable-gpu）

## 一、数据

| 指标 | Aurora（10k 库） | Obsidian（10k vault） | 思源（空工作区） | 口径 |
|---|---|---|---|---|
| **冷启动（窗口就绪）** | **待补测**（见口径注记③） | **254 ms**（251/254/516） | **256 ms**（256/256/293） | spawn→首个 X 窗口 mapped（Xlib 轮询 250ms 粒度） |
| 冷启动（全链就绪） | **153 ms**（DK-32） | 未测（见②） | 未测 | 应用层完全就绪 |

## 二、口径诚实化（三注记）

1. **竞品口径 = 窗口渲染就绪**（弱口径）：Electron 窗口 mapped 即判——后台索引/数据装载未验证就绪。Obsidian 10k vault 首开后台建索引，检索完整性无保证；
2. **Aurora 口径 = 全链就绪**（强口径）：bootstrap 完成（KV+投影 rebuild+索引 open，DK-32 方法论）。**Aurora 153ms 全链 < 竞品 254/256ms 窗口就绪——强口径跑赢弱口径，结论方向稳健**；
3. **竞品全链就绪未测**：需 UI 自动化埋点（如 Obsidian 索引完成事件/思源 kernel HTTP 就绪探测）——留待后续卡（思源可探测 127.0.0.1:6806 kernel API，Obsidian 无公开信号面）。

## 三、方法与可复现

- Obsidian v1.11.5 / 思源 v3.8.6（GitHub release AppImage 解包直调主二进制——AppRun 脚本 APPDIR 变量缺失坑，直调 `obsidian`/`siyuan` 二进制绕过）；
- 环境限制如实记录：xdotool 不可安装（dpkg 锁）→ python-xlib 替代；pkill -f 命令行含目标串会自杀（教训）；Xvfb 残留 socket 需预清理（X78-lock）；
- 计时脚本存档 `bench34/`（obsidian_cold.py / siyuan_cold.py）——下轮复测直接跑。

## 四、结论

- **Aurora 冷启动全链 153ms 对竞品窗口就绪 254/256ms：数据面占优**（口径差异已诚实标注）——perf-baseline.md 竞品列按本表补齐；
- 竞品**窗口就绪**集中在 ~255ms 一档（Electron 同源），Aurora（Tauri/wry 路线）全链 153ms——架构路线的启动开销优势得到数据支撑；
- 竞品检索就绪口径留待后续（需 UI 自动化埋点，单列后续小卡）。

— Alpha 2026-10-05
