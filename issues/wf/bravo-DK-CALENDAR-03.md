# bravo-DK-CALENDAR-03 — 重复日程引擎回执

**执行**: bravo ｜ 2026-10-30 ｜ 本 commit（Rust 引擎+TS 镜像+UI 消费+编辑器）

## 交付面（对照开工令四任务）

| 任务 | 状态 | 落点 |
|---|---|---|
| 1 RepeatRule 子集 | ✅ | entity 扩展 5 Optional 字段（by_weekday/day_of_month/count/lunar_month/lunar_day——serde default 向后兼容;02 三字段旧 JSON 不破坏） |
| 2 农历生日 | ✅ | 前端 repeatUtil.ts lunarToSolarYear（Lunar→Solar 反推;闰月策略=LunarYear.getLeapMonth 查表,无对应闰月→次月同日——文档化+测试锁定） |
| 3 展开引擎 | ✅ | core repeat_engine.rs `expand(rule, anchor, from, to)`——惰性逐周期+早停;EXPAND_LIMIT=500+迭代硬顶 10k |
| 4 月历/日视图消费 | ✅ | CalendarView monthInstances/selEvents 实例化（hasMark/dayEventDots 角标+dayAxis 实例落轴;跨天单次 02 语义保留;实例 key=base_id+offset） |

## 验证矩阵

- **Rust 单测 12/12**（本机真跑:月末 31→短月 28/闰年 2-29→2-28/跨年/interval=2/
  count 含范围前消耗/until/500 上限/农历分流空/interval=0 防御/范围前空）
- **Playwright 6/6**（repro-cal03.js:每周三周角标+次月 4 格/农历生日 2024-02-10→
  2025-01-29+2026-02-17（DoD 闰月年用例）/月末钳 2026-02-28/双监听 0）
- 回归: CALENDAR-02 7/7 + CALENDAR-01 6/6 无砸
- clippy -D warnings=0;fmt --check=0;tsc=0;build 10.6MB
- UI: EventEditorSheet 重复段（freq 六选含「农历每年」/间隔/周几条/次数/
  农历锚点提示——Lunar.fromDate(start) 反推月日）

## 架构分工（文档化）

- **公历子集 Rust/TS 双实现**（语义镜像——单测双锁定;Rust 供后续桌面/FFI 面）
- **农历仅 TS**（lunar-typescript 库现成;Rust 侧 yearly+lunar 返回空集——
  开工令点名 lunar-typescript 反推）
- count=锚点起总次数（含范围前已发生）;until/count 先到为准——行业 RRULE 惯例

## 坑实录

- **测试脚本字典序翻月 bug**：「2026 年 10 月」<「2026 年 2 月」（字符 '1'<'2'）
  → 反方向死翻——年月比较必须数字序
- **锚时区语义**：注入 start='2026-01-31T20:00Z' 本地=2-01 04:00（锚月已跨）——
  测试注入须按「本地锚意图」折 UTC（12:00Z=本地 20:00）
- lunar-typescript getLeapMonth 在 **LunarYear 类**（Lunar 上无——查 .d.ts 定位）
- fmt 手工修 root 属主文件（cargo fmt 写不进 Write 产物——逐 diff 愿望 Edit 落）

——bravo 10-30 14:5x
