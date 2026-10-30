//! # DK-CALENDAR-03 重复日程展开引擎
//!
//! 惰性按查询范围展开——不预生成全序列（锚点起逐周期推进+早停）。
//!
//! **架构分工**（开工令 2026-10-29）:
//! - Rust 侧: 公历子集（daily/weekly/monthly/yearly）——本模块
//! - 农历生日（freq=yearly+lunar 锚）: Rust 返回空集，由前端
//!   lunar-typescript Lunar→Solar 反推（闰月策略: 目标年无对应闰月→
//!   取次月同日——行业惯例）
//!
//! **行业惯例**（文档化——单测锁定）:
//! - 月末 31 → 短月（2 月 28/29、小月 30）钳制到月末
//! - 2-29 → 非闰年 2-28
//! - count = 从锚点起的总发生次数（含范围前已发生的;锚点=第 1 次）
//! - until 与 count 先到为准
//!
//! **上限保护**: 单次展开返回 ≤ [`EXPAND_LIMIT`]（500）条;迭代器另有
//! 硬顶防无限 repeat+远锚点死循环。

use aurora_entity::{RepeatFreq, RepeatRule};
use chrono::{DateTime, Datelike, Duration, Utc};

/// 单次展开返回条数上限（DoD: 防炸）。
pub const EXPAND_LIMIT: usize = 500;
/// 迭代硬顶（远锚点+无限规则的防死循环界;正常查询远触不到）。
const ITER_HARD_CAP: usize = 10_000;

/// 展开 [from, to] 范围内的重复实例时刻。
///
/// 返回按时间升序的 UTC 时刻列表（实例时刻 = 锚点时分秒 + 展开日）。
/// 农历模式（yearly + lunar_month Some）返回空——前端反推。
/// `to <= from` 或 `to <= anchor` 语义上无实例可展开（返回空）。
pub fn expand(
    rule: &RepeatRule,
    anchor: DateTime<Utc>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    if rule.interval == 0 || to <= from {
        return Vec::new();
    }
    // 农历生日: Rust 引擎不展开（前端 lunar-typescript 反推——见模块注记）
    if rule.freq == RepeatFreq::Yearly && rule.lunar_month.is_some() {
        return Vec::new();
    }
    let mut out: Vec<DateTime<Utc>> = Vec::new();
    let mut hits_from_anchor: u32 = 0; // count 语义: 锚点起总发生次数
    let mut iterated = 0usize;
    for candidate in candidates(rule, anchor) {
        iterated += 1;
        if iterated > ITER_HARD_CAP {
            tracing::warn!("repeat expand iter hard cap hit — anchor {anchor}");
            break;
        }
        hits_from_anchor += 1;
        if let Some(c) = rule.count {
            if hits_from_anchor > c {
                break; // count 截断: 后续全不会发生
            }
        }
        if candidate > to {
            break; // 升序生成——首个越界即可停
        }
        if candidate < from {
            continue; // 范围前发生（count 消耗已计）
        }
        if let Some(u) = rule.until {
            if candidate > u {
                break;
            }
        }
        if out.len() >= EXPAND_LIMIT {
            break;
        }
        out.push(candidate);
    }
    out
}

/// 候选时刻升序生成器（锚点=第 1 个候选）。
fn candidates(rule: &RepeatRule, anchor: DateTime<Utc>) -> impl Iterator<Item = DateTime<Utc>> {
    let time = anchor.time();
    let interval = rule.interval.max(1) as i64;
    match rule.freq {
        RepeatFreq::Daily => Box::new((0i64..).map(move |n| anchor + Duration::days(n * interval)))
            as Box<dyn Iterator<Item = DateTime<Utc>>>,
        RepeatFreq::Weekly => weekly_candidates(rule, anchor, time, interval),
        RepeatFreq::Monthly => monthly_candidates(rule, anchor, time, interval),
        RepeatFreq::Yearly => yearly_candidates(rule, anchor, time, interval),
    }
}

/// 周生成: 锚点所在自然周（周一界）为第 1 周;每 interval 周的
/// by_weekday（None/空=锚点周几）日发生。
fn weekly_candidates(
    rule: &RepeatRule,
    anchor: DateTime<Utc>,
    time: chrono::NaiveTime,
    interval: i64,
) -> Box<dyn Iterator<Item = DateTime<Utc>>> {
    let weekdays: Vec<u8> = match &rule.by_weekday {
        Some(ws) if !ws.is_empty() => ws.to_vec(),
        _ => vec![anchor.weekday().num_days_from_monday() as u8],
    };
    // 锚点周界（周一;Copy 双份——map/filter 各自 move）
    let week_start =
        anchor.date_naive() - Duration::days(anchor.weekday().num_days_from_monday() as i64);
    let week_start2 = week_start;
    let anchor_tz = Utc;
    Box::new(
        (0i64..) // 自锚点周界起逐日推进
            .map(move |d| week_start + Duration::days(d))
            .filter(move |date| {
                let week_idx = date.signed_duration_since(week_start2).num_days() / 7;
                if week_idx % interval != 0 {
                    return false;
                }
                weekdays.contains(&(date.weekday().num_days_from_monday() as u8))
            })
            .filter_map(move |date| date.and_time(time).and_local_timezone(anchor_tz).single()),
    )
}

/// 月生成: 锚点月序推进;日 = day_of_month（None=锚点日）;短月钳月末。
fn monthly_candidates(
    rule: &RepeatRule,
    anchor: DateTime<Utc>,
    time: chrono::NaiveTime,
    interval: i64,
) -> Box<dyn Iterator<Item = DateTime<Utc>>> {
    let anchor_day = rule.day_of_month.unwrap_or(anchor.day() as u8) as u32;
    let (y0, m0) = (anchor.year(), anchor.month());
    Box::new((0i64..).filter_map(move |n| {
        // 月序推进（跨年自然进位）
        let total = y0 as i64 * 12 + m0 as i64 - 1 + n * interval;
        let y = total.div_euclid(12) as i32;
        let m = (total.rem_euclid(12) + 1) as u32;
        let dim = days_in_month(y, m);
        let day = anchor_day.min(dim); // 短月钳制
        chrono::NaiveDate::from_ymd_opt(y, m, day)
            .map(|d| d.and_time(time))
            .and_then(|dt| dt.and_local_timezone(Utc).single())
    }))
}

/// 年生成: 锚点年序推进;2-29 非闰年钳 2-28（月末惯例同 monthly）。
fn yearly_candidates(
    rule: &RepeatRule,
    anchor: DateTime<Utc>,
    time: chrono::NaiveTime,
    interval: i64,
) -> Box<dyn Iterator<Item = DateTime<Utc>>> {
    let (y0, m0, d0) = (anchor.year(), anchor.month(), anchor.day());
    let _ = rule; // yearly 无附加字段（农历在前置分流已返回空）
    Box::new((0i64..).filter_map(move |n| {
        let y = (y0 as i64 + n * interval) as i32;
        let dim = days_in_month(y, m0);
        let day = d0.min(dim);
        chrono::NaiveDate::from_ymd_opt(y, m0, day)
            .map(|d| d.and_time(time))
            .and_then(|dt| dt.and_local_timezone(Utc).single())
    }))
}

fn days_in_month(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let first_next = chrono::NaiveDate::from_ymd_opt(ny, nm, 1).expect("valid date");
    let first = chrono::NaiveDate::from_ymd_opt(y, m, 1).expect("valid date");
    (first_next - first).num_days() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_entity::RepeatRule;
    use chrono::TimeZone;

    fn dt(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
    }

    fn rule(freq: RepeatFreq, interval: u32) -> RepeatRule {
        RepeatRule {
            freq,
            interval,
            until: None,
            by_weekday: None,
            day_of_month: None,
            count: None,
            lunar_month: None,
            lunar_day: None,
        }
    }

    /// ① 基础: weekly 锚点周几三连周（DoD UI 断言同源逻辑）。
    #[test]
    fn weekly_three_weeks() {
        // 2026-10-30 是周五;每周五×3 周（10-30 / 11-06 / 11-13）
        // to 收在 11-13 深夜——11-20（第 4 周五）不在窗内
        let a = dt(2026, 10, 30, 9);
        let out = expand(&rule(RepeatFreq::Weekly, 1), a, a, dt(2026, 11, 13, 23));
        assert_eq!(
            out,
            vec![dt(2026, 10, 30, 9), dt(2026, 11, 6, 9), dt(2026, 11, 13, 9)]
        );
    }

    /// ② by_weekday 多日: 周一+周四（interval 1）。
    #[test]
    fn weekly_by_weekday_multi() {
        // to=11-02 深夜——11-05（次周四）不在窗内
        let a = dt(2026, 10, 26, 8); // 周一
        let r = RepeatRule {
            by_weekday: Some(vec![0, 3]),
            ..rule(RepeatFreq::Weekly, 1)
        };
        let out = expand(&r, a, a, dt(2026, 11, 2, 23));
        assert_eq!(
            out,
            vec![dt(2026, 10, 26, 8), dt(2026, 10, 29, 8), dt(2026, 11, 2, 8)]
        );
    }

    /// ③ 月末 31 → 短月钳制（DoD: 1-31 → 2 月 28）。
    #[test]
    fn monthly_31_clamps_to_short_month() {
        let a = dt(2026, 1, 31, 10);
        let out = expand(&rule(RepeatFreq::Monthly, 1), a, a, dt(2026, 3, 31, 23));
        assert_eq!(
            out,
            vec![
                dt(2026, 1, 31, 10),
                dt(2026, 2, 28, 10),
                dt(2026, 3, 31, 10)
            ]
        );
    }

    /// ④ 闰年 2-29 → 非闰年 2-28（DoD: 闰月/闰年边界）。
    #[test]
    fn yearly_feb29_clamps_non_leap() {
        let a = dt(2024, 2, 29, 12);
        let out = expand(&rule(RepeatFreq::Yearly, 1), a, a, dt(2026, 12, 31, 23));
        assert_eq!(
            out,
            vec![
                dt(2024, 2, 29, 12),
                dt(2025, 2, 28, 12),
                dt(2026, 2, 28, 12)
            ]
        );
    }

    /// ⑤ 跨年: monthly 12-15 → 次年 1-15。
    #[test]
    fn monthly_crosses_year() {
        let a = dt(2026, 12, 15, 9);
        let out = expand(&rule(RepeatFreq::Monthly, 1), a, a, dt(2027, 1, 31, 23));
        assert_eq!(out, vec![dt(2026, 12, 15, 9), dt(2027, 1, 15, 9)]);
    }

    /// ⑥ 间隔 >1: 每 2 周。
    #[test]
    fn weekly_interval_2() {
        let a = dt(2026, 10, 30, 9); // 周五
        let out = expand(&rule(RepeatFreq::Weekly, 2), a, a, dt(2026, 11, 27, 23));
        assert_eq!(
            out,
            vec![
                dt(2026, 10, 30, 9),
                dt(2026, 11, 13, 9),
                dt(2026, 11, 27, 9)
            ]
        );
    }

    /// ⑦ count 截断: 锚点起共 3 次（含范围前发生——count 消耗语义）。
    #[test]
    fn count_truncation_includes_pre_range_hits() {
        let a = dt(2026, 10, 1, 9);
        let r = RepeatRule {
            count: Some(3),
            ..rule(RepeatFreq::Daily, 1)
        };
        // 范围从 10-03 起——10-01/02 已发生（count 消耗 2）→仅 10-03 落范围
        let out = expand(&r, a, dt(2026, 10, 3, 0), dt(2026, 10, 31, 23));
        assert_eq!(out, vec![dt(2026, 10, 3, 9)]);
    }

    /// ⑧ until 截断（与 count 先到为准——单测 until 单独）。
    #[test]
    fn until_truncation() {
        let a = dt(2026, 10, 1, 9);
        let r = RepeatRule {
            until: Some(dt(2026, 10, 3, 9)),
            ..rule(RepeatFreq::Daily, 1)
        };
        let out = expand(&r, a, a, dt(2026, 10, 31, 23));
        assert_eq!(
            out,
            vec![dt(2026, 10, 1, 9), dt(2026, 10, 2, 9), dt(2026, 10, 3, 9)]
        );
    }

    /// ⑨ 500 条上限（DoD 防炸）。
    #[test]
    fn expand_limit_500() {
        let a = dt(2000, 1, 1, 0);
        let out = expand(&rule(RepeatFreq::Daily, 1), a, a, dt(2100, 1, 1, 0));
        assert_eq!(out.len(), 500, "单次展开硬上限 500");
    }

    /// ⑩ 农历模式: Rust 返回空（前端 lunar-typescript 反推——架构分工注记）。
    #[test]
    fn lunar_mode_returns_empty() {
        let a = dt(2024, 2, 10, 9);
        let r = RepeatRule {
            lunar_month: Some(1),
            lunar_day: Some(1),
            ..rule(RepeatFreq::Yearly, 1)
        };
        let out = expand(&r, a, a, dt(2026, 12, 31, 23));
        assert!(out.is_empty(), "农历=前端反推,Rust 面空集");
    }

    /// ⑪ interval=0 防御: 空集。
    #[test]
    fn zero_interval_defensive() {
        let a = dt(2026, 10, 30, 9);
        assert!(expand(&rule(RepeatFreq::Daily, 0), a, a, dt(2026, 11, 30, 9)).is_empty());
    }

    /// ⑫ 范围不含锚点也不含后续: 空。
    #[test]
    fn range_before_anchor_empty() {
        let a = dt(2026, 10, 30, 9);
        assert!(expand(
            &rule(RepeatFreq::Daily, 1),
            a,
            dt(2026, 1, 1, 0),
            dt(2026, 1, 31, 23)
        )
        .is_empty());
    }
}
