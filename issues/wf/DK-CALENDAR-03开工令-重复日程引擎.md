# DK-CALENDAR-03 开工令 · 重复日程引擎（核-2）

**派发**: Leo 2026-10-29 22:08 ｜ **Assignee**: Bravo ｜ **Estimate**: 2-4 天 ｜ **Labels**: `area/entity`, `area/mobile-calendar`, `P1`, 依赖: CALENDAR-02

## 背景

评估置信 70%——日历学边界（闰月/短月/跨年）是真坑，单测覆盖是本卡灵魂。

## 任务

1. **RepeatRule 子集**（core 展开+存储两层）:
   - none / daily / weekly（选周几集合）/ monthly（锚日）/ yearly
   - 间隔（每 N 单位）+ 结束: 永不 / 次数 / 截至
2. **农历生日**: `yearly_lunar` 模式——每年农历月日反推公历（lunar-typescript Lunar→Solar 反查; **闰月策略**: 目标年无对应闰月→取次月同日（行业惯例（文档化
3. **展开引擎**（Rust core `expand(rule, range) -> Vec<DateTimeUtc>`: 惰性按查询范围展开（不预生成全序列; 上限保护（单次展开 ≤ 500 条防炸
4. **月历/日视图消费**: 展开结果并入当日角标/时间轴（与单次事件混排去重（实例 key=base_id+offset

## DoD

- [ ] 单测边界: 月末 31→短月、闰月年、跨年、间隔>1、次数截断、500 条上限
- [ ] 农历生日: 生日 2024-02-10（正月初一→2025/2026 展开正确（闰月年用例必测
- [ ] UI 断言: 建每周重复→连续三周角标可见
- [ ] CI 五门绿

---

## P2 待用户拍板（未派单）

- **前台提醒**（app 内通知（1-2 天）
- **后台准时提醒**（原生层 3-5 天+真机联调环（定位笔记 app 下是否需要——涉及原生 Notification/AlarmManager 从零起步（建议裁决时权衡: Task due 提醒与 Event 提醒是否同批做
