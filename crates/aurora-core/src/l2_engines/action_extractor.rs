//! GTD 2.0 行动项提取 — V20 Phase 3（「行动项提取抽检 ≥80%」）
//!
//! 从笔记正文中提取可执行行动项并落入任务投影（GTD 闭环）:
//!
//! ```text
//! 笔记正文 ──extract_action_items──▶ Vec<ActionItem>
//!            （本地规则引擎，零依赖零延迟）
//!                 │
//!                 ▼ apply_to_projection
//!           TaskProjection（by_status("inbox") → GTD 收件箱）
//!                 │
//!                 ▼ TodayView（today/stats 聚合）
//! ```
//!
//! # 提取规则（本地引擎 — Phase 3 先落地确定性基线）
//!
//! | 模式 | 示例 | 状态 |
//! |------|------|------|
//! | Markdown 任务 `- [ ]` | `- [ ] 整理会议纪要` | inbox |
//! | 中文明确动词句式 | `明天提交报告`（动词表命中） | inbox |
//! | checkbox 未完成 | `[ ] 复核预算` | inbox |
//! | 截止日期词 | `周五前`/`明天`/`下周` | 解析 due_date |
//! | 优先级词 | `紧急`/`尽快` | urgent/high |
//!
//! `- [x]` 已完成项 → status=done（不丢历史）。
//!
//! AI 液化（AIProvider 路径）作为后续增强: 规则引擎先保证离线可用与
//! 确定性（V20 本地优先原则 — AI 永远是增强而非前提）。

use chrono::{Datelike, Duration, Utc};

use crate::l2_engines::task_projection::TaskProjection;

/// 提取出的行动项。
#[derive(Debug, Clone, PartialEq)]
pub struct ActionItem {
    /// 行动项文本（已修剪标记前缀）。
    pub title: String,
    /// GTD 初始状态（inbox / done）。
    pub status: &'static str,
    /// 优先级: low/medium/high/urgent。
    pub priority: &'static str,
    /// 截止（Unix epoch 毫秒; 无期限为 None）。
    pub due_date: Option<i64>,
}

/// 行动项提取器（本地规则引擎）。
pub struct ActionItemExtractor;

/// 中文行动动词表（启发式 — 命中即视为行动句）。
const ACTION_VERBS: &[&str] = &[
    "提交", "发送", "回复", "整理", "完成", "确认", "预约", "购买", "预定", "联系", "电话", "汇报",
    "写", "修改", "审核", "复核", "部署", "上线", "修复", "测试", "发布", "准备", "安排", "报名",
    "缴费", "续费", "归还",
];

/// 紧急词 → 优先级。
fn priority_of(text: &str) -> &'static str {
    if text.contains("紧急") || text.contains("立刻") || text.contains("马上") {
        "urgent"
    } else if text.contains("尽快") || text.contains("优先") || text.contains("重要") {
        "high"
    } else {
        "medium"
    }
}

/// R3-1：日期解析结果（前端实时预览所需——命中片段 span 用于 chip 标注）。
#[derive(Debug, Clone, PartialEq)]
pub struct DueParse {
    /// 截止时刻（Unix 毫秒）
    pub due_ts: i64,
    /// 命中片段 **char offset** `[matched_start, matched_end)`（text 内——
    /// 天词/周词/N天后 与 时刻段 的并集）。char 而非字节：前端 JS 字符串
    /// 切片按 UTF-16/code point 走（BMP 内 char==code unit），字节偏移会
    /// 漂移（R-α1 复核采纳——v2 overlay 高亮的正确坐标系）。
    pub matched_start: usize,
    pub matched_end: usize,
}

/// 相对期限词 → 截止时间 + 命中片段（DK-3.4 v2 + R3-1 span 化）。
///
/// 语义与 v2 全同（span 化仅**附加**区间信息，due_ts 计算零回归）：
/// - 天词：今天/明天/后天/大后天/N 天后/本周/下周/月底
/// - 周词：周一~周日 → **下一个该周几**（按今天 weekday 环算，含今天）
/// - 时刻：`N 点`/`N点半`/`N点M分`/`N:MM`/`N:MM 分`——与天词组合；
///   单独出现=今天该时刻（已过则明天的）
/// - 无时刻的天词：v1 语义 now+days（相对偏移，零回归）
pub fn due_parse(text: &str) -> Option<DueParse> {
    due_parse_at(text, Utc::now())
}

/// R-α2：now 注入变体——对拍测试可固定时刻断言精确值（相对词
/// 「明天/周X/N天后」以 now 为基准；due_parse=薄壳注入 Utc::now()）。
pub fn due_parse_at(text: &str, now: chrono::DateTime<Utc>) -> Option<DueParse> {
    // 时刻解析（局部辅助）：返回 (时, 分)
    fn parse_time(s: &str) -> Option<(u32, u32)> {
        // N:MM / N:MM分
        if let Some(pos) = s.find(':') {
            let h: u32 = s.get(..pos)?.trim().parse().ok()?;
            let m: u32 = s
                .get(pos + 1..)?
                .trim_end_matches(['分', ' '])
                .parse()
                .ok()?;
            if h < 24 && m < 60 {
                return Some((h, m));
            }
            return None;
        }
        // N点半 / N点M分 / N点（"9点半"→(9,30) / "9点15"→(9,15) / "9点"→(9,0)）
        let half = s.contains("点半");
        let base: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
        let h: u32 = base.parse().ok()?;
        if h > 23 {
            return None;
        }
        if half {
            return Some((h, 30));
        }
        // N点M分
        if let Some(p) = s.find("点") {
            let rest = &s[p + 3..];
            let m: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !m.is_empty() {
                let m: u32 = m.parse().ok()?;
                if m < 60 {
                    return Some((h, m));
                }
            }
        }
        Some((h, 0))
    }

    // 1) 组合形态：天词 + 时刻（「明天 9 点」「周五3点」「3 天后 14:30」）
    //    朴素扫描：找时刻标记（点/:），向前回扫数字段、向后吃时刻字，
    //    截出时刻片段；解析失败视为无时刻（纯散文里含「点」不误伤）。
    //    R3-1：保留片段 span（尾空白回退对齐 trim 边界）。
    let time_seg: Option<(String, usize, usize)> = (|| {
        let bytes = text.as_bytes();
        let idx = text.rfind(['点', ':', '：'])?;
        // 向前回扫数字与空白
        let mut start = idx;
        while start > 0 {
            let c = text[..start].chars().last().unwrap();
            if c.is_ascii_digit() || c == ' ' {
                start -= c.len_utf8();
            } else {
                break;
            }
        }
        // 向后吃时刻字符（点/半/分/冒号/数字/空格）
        let mut end = idx + text[idx..].chars().next().map_or(1, char::len_utf8);
        while end < bytes.len() {
            let c = text[end..].chars().next().unwrap();
            if c.is_ascii_digit() || ['点', '半', '分', ':', '：', ' '].contains(&c) {
                end += c.len_utf8();
            } else {
                break;
            }
        }
        // 尾空白回退（对齐 trim 边界——span 精确）
        let seg_raw = &text[start..end];
        let end = end - (seg_raw.len() - seg_raw.trim_end().len());
        let seg_start = start + (seg_raw.len() - seg_raw.trim_start().len());
        let seg = text[seg_start..end].trim();
        if seg.is_empty() || parse_time(seg).is_none() {
            return None;
        }
        Some((seg.to_string(), seg_start, end))
    })();
    let time_str = time_seg.as_ref().map(|(s, _, _)| s.clone());

    // 2) 天词解析 → (基准日偏移, 命中 span)。R3-1：find 定位取区间。
    let day_hit: Option<(i64, usize, usize)> = if text.contains("大后天") {
        text.find("大后天").map(|p| (3, p, p + "大后天".len()))
    } else if text.contains("后天") {
        text.find("后天").map(|p| (2, p, p + "后天".len()))
    } else if text.contains("明天") || text.contains("明日") {
        text.find("明天").or_else(|| text.find("明日")).map(|p| {
            let wlen = if text[p..].starts_with("明日") {
                "明日".len()
            } else {
                "明天".len()
            };
            (1, p, p + wlen)
        })
    } else if text.contains("今天") || text.contains("今日") {
        text.find("今天").or_else(|| text.find("今日")).map(|p| {
            let wlen = if text[p..].starts_with("今日") {
                "今日".len()
            } else {
                "今天".len()
            };
            (0, p, p + wlen)
        })
    } else if text.contains("本周") || text.contains("这周") {
        text.find("本周").or_else(|| text.find("这周")).map(|p| {
            let wlen = if text[p..].starts_with("这周") {
                "这周".len()
            } else {
                "本周".len()
            };
            (5, p, p + wlen)
        })
    } else if text.contains("下周") {
        text.find("下周").map(|p| (8, p, p + "下周".len()))
    } else if text.contains("月底") {
        text.find("月底").map(|p| (20, p, p + "月底".len()))
    } else if let Some(idx) = text.find("天后") {
        // N 天后（数字在「天后」前——回扫数字段，允许中间空白「3 天后」）
        let head = &text[..idx];
        let trimmed = head.trim_end();
        let n_start = trimmed.len()
            - trimmed
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .map(char::len_utf8)
                .sum::<usize>();
        let n: String = trimmed[n_start..].to_string();
        if n.is_empty() {
            None // 裸「天后」无数字——不解析
        } else {
            n.parse::<i64>()
                .ok()
                .map(|d| (d.min(3650), n_start, idx + "天后".len()))
        }
    } else {
        None
    };
    let day_offset = day_hit.as_ref().map(|(d, _, _)| *d);
    let day_span = day_hit.as_ref().map(|(_, s, e)| (*s, *e));

    // 3) 周词解析（下一个该周几——按今天 weekday 环算，含今天=0 天）。
    //    R3-1：附命中 span。
    let today = now.weekday().num_days_from_monday() as i64;
    let weekday_hit: Option<(i64, usize, usize)> = [
        ("周一", chrono::Weekday::Mon),
        ("周二", chrono::Weekday::Tue),
        ("周三", chrono::Weekday::Wed),
        ("周四", chrono::Weekday::Thu),
        ("周五", chrono::Weekday::Fri),
        ("周5", chrono::Weekday::Fri),
        ("周六", chrono::Weekday::Sat),
        ("周日", chrono::Weekday::Sun),
        ("周天", chrono::Weekday::Sun),
    ]
    .iter()
    .find_map(|(w, wd)| {
        text.find(w).map(|p| {
            let target = wd.num_days_from_monday() as i64;
            ((target - today + 7) % 7, p, p + w.len())
        })
    });
    let weekday_offset = weekday_hit.as_ref().map(|(d, _, _)| *d);
    let weekday_span = weekday_hit.as_ref().map(|(_, s, e)| (*s, *e));

    // 4) 合成：时刻存在→那天该时刻（00:00 起 + 天数 + 时分）；
    //    无时刻→v1 相对偏移语义（now+days，零回归）。
    //    R3-1：span = 命中片段并集（天词/周词/N天后 与 时刻段）。
    let (h, m) = time_str.as_deref().and_then(parse_time).unzip();
    // R-α1：字节偏移 → char offset（内部 span 全程字节算，出口统一转——
    // 前端 JS 切片按 code point 走，中文 UTF-8 3 字节/char 直接漂移）
    let to_char = |b: usize| text[..b].chars().count();
    let span = || {
        let mut spans: Vec<(usize, usize)> = Vec::new();
        if let Some(s) = day_span {
            spans.push(s);
        }
        if let Some(s) = weekday_span {
            spans.push(s);
        }
        if let Some((_, s, e)) = time_seg.as_ref() {
            spans.push((*s, *e));
        }
        let lo = spans.iter().map(|(s, _)| *s).min().unwrap_or(0);
        let hi = spans.iter().map(|(_, e)| *e).max().unwrap_or(0);
        (to_char(lo), to_char(hi))
    };
    if let Some(days) = day_offset.or(weekday_offset) {
        let base = now.date_naive() + chrono::Duration::days(days);
        let dt = base
            .and_hms_opt(h.unwrap_or(23), m.unwrap_or(59), 59)
            .unwrap_or_else(|| base.and_hms_opt(23, 59, 59).expect("23:59:59 valid"));
        // 无时刻的天词=相对偏移（v1 语义）；有时刻=当天该时刻
        let due_ts = if time_str.is_some() {
            dt.and_utc().timestamp_millis()
        } else {
            (now + Duration::days(days)).timestamp_millis()
        };
        let (matched_start, matched_end) = span();
        return Some(DueParse {
            due_ts,
            matched_start,
            matched_end,
        });
    }
    // 仅时刻（无天词）：今天该时刻，已过则明天
    if let Some((h, m)) = time_str.as_deref().and_then(parse_time) {
        let today_at = now
            .date_naive()
            .and_hms_opt(h, m, 0)
            .expect("h/m validated");
        let dt = today_at.and_utc();
        let dt = if dt < now { dt + Duration::days(1) } else { dt };
        let (matched_start, matched_end) = span();
        return Some(DueParse {
            due_ts: dt.timestamp_millis(),
            matched_start,
            matched_end,
        });
    }
    None
}

/// 兼容包装（R3-1 前签名）——现有调用面零改动。
fn due_date_of(text: &str) -> Option<i64> {
    due_parse(text).map(|d| d.due_ts)
}

impl ActionItemExtractor {
    /// 从笔记正文提取行动项（Markdown 任务 + 中文动词句式）。
    ///
    /// 每行独立判定; `- [ ]`/`- [x]`/`* [ ]` 前缀剥离;
    /// 普通行需命中动词表才视为行动项（宁缺毋滥 — 抽检精度优先）。
    pub fn extract(text: &str) -> Vec<ActionItem> {
        let mut items = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();

            // Markdown 任务项
            if let Some(rest) = trimmed
                .strip_prefix("- [ ]")
                .or_else(|| trimmed.strip_prefix("* [ ]"))
                .or_else(|| trimmed.strip_prefix("+ [ ]"))
                .or_else(|| trimmed.strip_prefix("[ ]"))
            {
                let title = rest.trim().to_string();
                if !title.is_empty() {
                    items.push(ActionItem {
                        priority: priority_of(&title),
                        status: "inbox",
                        due_date: due_date_of(&title),
                        title,
                    });
                }
                continue;
            }
            if let Some(rest) = trimmed
                .strip_prefix("- [x]")
                .or_else(|| trimmed.strip_prefix("- [X]"))
                .or_else(|| trimmed.strip_prefix("* [x]"))
                .or_else(|| trimmed.strip_prefix("[x]"))
            {
                let title = rest.trim().to_string();
                if !title.is_empty() {
                    items.push(ActionItem {
                        priority: priority_of(&title),
                        status: "done",
                        due_date: due_date_of(&title),
                        title,
                    });
                }
                continue;
            }

            // 普通行: 动词表命中（且长度合理 — 排除标题/表格行）
            let len = trimmed.chars().count();
            if (4..=60).contains(&len)
                && !trimmed.starts_with('#')
                && !trimmed.starts_with('|')
                && !trimmed.starts_with('>')
                && ACTION_VERBS.iter().any(|v| trimmed.contains(v))
            {
                items.push(ActionItem {
                    priority: priority_of(trimmed),
                    status: "inbox",
                    due_date: due_date_of(trimmed),
                    title: trimmed.to_string(),
                });
            }
        }
        items
    }

    /// 提取并应用到任务投影（**note 级重建同步** — R2-0 修复）。
    ///
    /// v1 缺陷三重（行号派生 task_id 的结构性问题）：
    /// 1. 删行残留 — 笔记里删掉任务行后投影旧行永存；
    /// 2. 行号漂移 — 任务行前插入文本使 idx 全体 +1，同一任务
    ///    以新旧两个 id 双生（旧内容残行+新行）；
    /// 3. 勾选重置 — TodoView 勾完成（status=done）不回写文本，
    ///    下次保存重播种 status=inbox 静默撤销用户操作。
    ///
    /// v2 语义（**以笔记文本为权威的 title 键 diff 同步**）：
    /// 快照旧 ai:* 行 → 清场 → 按 title 匹配保留终态（done 优先 —
    /// 用户显式完成不因编辑器文本未同步而丢失）→ 重播种。
    /// 同笔记同 title 多行=各占独立行（idx 后缀仍保稳定（同步轮内），
    /// 匹配仅用于终态保留）。
    pub async fn apply_to_projection(
        note_id: &str,
        text: &str,
        projection: &TaskProjection,
    ) -> Result<usize, crate::Error> {
        // 1) 快照旧 ai:* 行的完整状态（title → status 映射）——
        //    忠实保留用户在 TodoView 的最近显式操作（勾完成=done 保留，
        //    取消勾=inbox 保留不被文本 `- [ ]` 重播回 done）。
        //    冲突策略：UI 交互态优先于文本标记（勾选是更近的显式操作）。
        let mut prev_status: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for r in projection.rows_for_note(note_id) {
            prev_status.entry(r.title).or_insert_with(|| r.status);
        }
        // 2) 清场（删行残留/行号漂移一并消除）
        projection.remove_rows_for_note(note_id);
        // 3) 重播种（终态保留：旧 done + 同 title → 播 done）
        let items = Self::extract(text);
        let n = items.len();
        for (idx, item) in items.into_iter().enumerate() {
            let status = prev_status
                .get(&item.title)
                .cloned()
                .unwrap_or_else(|| item.status.to_string());
            projection.seed_row(
                &format!("ai:{note_id}:{idx}"),
                note_id,
                &item.title,
                status.as_str(),
                item.priority,
                item.due_date,
            );
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::event_bus::layered::LayeredEventBus;
    use crate::event_bus::layered::{AppEvent, InMemoryEventQueue};
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use crate::l2_engines::task_projection::TaskProjection;

    fn make_projection() -> TaskProjection {
        TaskProjection::new(Arc::new(MemoryKVStore::default()), Box::new(Vec::new))
    }

    // ── DK-3.4 v2：自然语言日期（「明天 9 点」Todoist 语义） ──
    // 由于 due_date_of 以 Utc::now() 为基准，测试断言用相对关系而非绝对值。

    #[test]
    fn nl_date_tomorrow_with_time() {
        let due = due_date_of("明天 9 点交报告").expect("明天 9 点须解析");
        let dt = chrono::DateTime::<Utc>::from_timestamp_millis(due).unwrap();
        let tomorrow = (Utc::now() + Duration::days(1)).date_naive();
        assert_eq!(dt.date_naive(), tomorrow, "日期=明天");
        assert_eq!(dt.format("%H").to_string(), "09", "时刻=09 点");
        assert_eq!(dt.format("%M").to_string(), "00");
    }

    #[test]
    fn nl_date_half_and_colon() {
        let due = due_date_of("明天 2点半开会").expect("2点半");
        let dt = chrono::DateTime::<Utc>::from_timestamp_millis(due).unwrap();
        assert_eq!(dt.format("%H:%M").to_string(), "02:30");

        let due2 = due_date_of("周五 14:30 评审").expect("14:30");
        let dt2 = chrono::DateTime::<Utc>::from_timestamp_millis(due2).unwrap();
        assert_eq!(dt2.format("%H:%M").to_string(), "14:30");
        // 周五=下一个周五（weekday 环算——不再恒 3 天）
        let today = Utc::now().weekday().num_days_from_monday() as i64;
        let fri = chrono::Weekday::Fri.num_days_from_monday() as i64;
        let expect_days = (fri - today + 7) % 7;
        let expect_date = (Utc::now() + Duration::days(expect_days)).date_naive();
        assert_eq!(dt2.date_naive(), expect_date, "周五=下一个周五");
    }

    #[test]
    fn nl_date_days_later_and_bare_time() {
        // N 天后
        let due = due_date_of("3 天后交付").expect("3 天后");
        let expect = (Utc::now() + Duration::days(3)).timestamp_millis();
        assert!((due - expect).abs() < 60_000, "3 天后≈now+3d（±1min 容差）");
        // 仅时刻（无天词）：未来→今天；过去→明天
        let due2 = due_date_of("18:00 提交").expect("18:00");
        let dt2 = chrono::DateTime::<Utc>::from_timestamp_millis(due2).unwrap();
        let now = Utc::now();
        let ok = dt2.date_naive() == now.date_naive()
            || dt2.date_naive() == (now + Duration::days(1)).date_naive();
        assert!(ok, "仅时刻=今天或明天该时刻");
    }

    #[test]
    fn nl_date_prose_no_false_positive() {
        // 散文含「点」但无时刻语义 → 不误判（如「三点认识」会被解析——
        // 已是时刻语义可接受；此例验证纯噪声：无数字+无日期词）
        assert!(due_date_of("这个想法很有亮点，值得深挖").is_none());
        assert!(due_date_of("普通任务无日期").is_none());
    }

    // ── R2-0 修复回归：note 级重建同步（删行残留/行号漂移/勾选保留） ──

    #[tokio::test]
    async fn sync_rebuild_removes_stale_rows() {
        let proj = make_projection();
        // v1: 两行 → v2: 删一行+改一行 → 旧行不得残留
        ActionItemExtractor::apply_to_projection("n1", "- [ ] 任务甲\n- [ ] 任务乙", &proj)
            .await
            .unwrap();
        assert_eq!(proj.rows_for_note("n1").len(), 2);
        ActionItemExtractor::apply_to_projection("n1", "- [ ] 任务甲改", &proj)
            .await
            .unwrap();
        let rows = proj.rows_for_note("n1");
        assert_eq!(rows.len(), 1, "旧 idx 行清场——删行/漂移不残留");
        assert_eq!(rows[0].title, "任务甲改");
    }

    #[tokio::test]
    async fn sync_preserves_toggle_status_both_ways() {
        let proj = make_projection();
        ActionItemExtractor::apply_to_projection("n2", "- [ ] 写周报", &proj)
            .await
            .unwrap();
        // 用户在 TodoView 勾完成（toggle 面——直接改投影行模拟）
        let tid = proj.rows_for_note("n2")[0].task_id.clone();
        proj.seed_row(&tid, "n2", "写周报", "done", "medium", None);
        // 文本仍是 `- [ ]`（UI 勾选不回写）——重建必须保 done
        ActionItemExtractor::apply_to_projection("n2", "- [ ] 写周报", &proj)
            .await
            .unwrap();
        assert_eq!(
            proj.rows_for_note("n2")[0].status,
            "done",
            "勾完成不因保存重置"
        );
        // 反向：用户取消勾（done→inbox）——重建不得重播回 done
        let tid2 = proj.rows_for_note("n2")[0].task_id.clone();
        proj.seed_row(&tid2, "n2", "写周报", "inbox", "medium", None);
        ActionItemExtractor::apply_to_projection("n2", "- [ ] 写周报", &proj)
            .await
            .unwrap();
        assert_eq!(
            proj.rows_for_note("n2")[0].status,
            "inbox",
            "取消勾不被文本重播"
        );
    }

    // ── R3-1：due_parse span 化回归 ──

    // ── R-α2：now 注入对拍（固定时刻断言精确值） ──

    #[test]
    fn due_parse_at_fixed_now() {
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2026, 10, 21, 10, 0, 0).unwrap(); // 周三
                                                                         // 「明天 9 点」= 2026-10-22 09:00 精确断言
        let d = due_parse_at("明天 9 点交", now).unwrap();
        let dt = chrono::DateTime::from_timestamp_millis(d.due_ts).unwrap();
        assert_eq!(dt.format("%Y-%m-%d %H:%M").to_string(), "2026-10-22 09:00");
        // 「周五」按环算=本周五=2026-10-23（今天周三）
        let d2 = due_parse_at("周五前交", now).unwrap();
        let dt2 = chrono::DateTime::from_timestamp_millis(d2.due_ts).unwrap();
        assert_eq!(dt2.format("%Y-%m-%d").to_string(), "2026-10-23");
        // 「3 天后」=2026-10-24
        let d3 = due_parse_at("3 天后汇报", now).unwrap();
        let dt3 = chrono::DateTime::from_timestamp_millis(d3.due_ts).unwrap();
        assert_eq!(dt3.format("%Y-%m-%d").to_string(), "2026-10-24");
        // 「今天」=相对 now（now+0 天）
        let d4 = due_parse_at("今天内给", now).unwrap();
        let dt4 = chrono::DateTime::from_timestamp_millis(d4.due_ts).unwrap();
        assert_eq!(dt4.format("%Y-%m-%d").to_string(), "2026-10-21");
    }

    #[test]
    fn due_parse_span_combination() {
        // R-α1：span=char offset 语义——中文前缀不漂移（JS Array.from 对齐）
        let d = due_parse("- [ ] 交周报 明天 9 点").unwrap();
        let chars: Vec<char> = "- [ ] 交周报 明天 9 点".chars().collect();
        let m: String = chars[d.matched_start..d.matched_end].iter().collect();
        assert_eq!(m, "明天 9 点", "并集 span 覆盖天词+时刻段（char offset）");
        assert_eq!(
            &chars[..d.matched_start].iter().collect::<String>(),
            "- [ ] 交周报 "
        );
        // 有时刻=当天该时刻语义：非相对偏移（due 应为 09:00 锚定）
        let due_local = chrono::DateTime::from_timestamp_millis(d.due_ts).unwrap();
        assert_eq!(due_local.format("%H:%M").to_string(), "09:00");
    }

    #[test]
    fn due_parse_span_weekday_and_ndafter() {
        // R-α1 char 语义切片助手
        let seg = |text: &str, d: &DueParse| -> String {
            text.chars().collect::<Vec<_>>()[d.matched_start..d.matched_end]
                .iter()
                .collect()
        };
        // 周词 span
        let d = due_parse("周五 3 点 前完成").unwrap();
        assert_eq!(seg("周五 3 点 前完成", &d), "周五 3 点");
        // N 天后（数字回扫含空白）span 覆盖「3 天后 14:30」
        let d2 = due_parse("3 天后 14:30 汇报").unwrap();
        assert_eq!(seg("3 天后 14:30 汇报", &d2), "3 天后 14:30");
        // 「记得 交周五3点」——中文前缀 char offset 不漂移
        let d3 = due_parse("记得 交周五3点").unwrap();
        assert_eq!(seg("记得 交周五3点", &d3), "周五3点");
    }

    #[test]
    fn due_parse_span_compat_wrapper() {
        // 兼容包装与 span 版 due_ts 一致
        let t = "明天 9 点交";
        assert_eq!(due_date_of(t), due_parse(t).map(|d| d.due_ts));
        // 无命中 → None
        assert!(due_parse("普通散文无日期").is_none());
    }

    #[test]
    fn extracts_markdown_tasks_with_metadata() {
        let text = "# 会议纪要\n- [ ] 整理今天的会议记录\n- [x] 发送周报\n普通段落";
        let items = ActionItemExtractor::extract(text);
        assert_eq!(items.len(), 2);

        assert_eq!(items[0].title, "整理今天的会议记录");
        assert_eq!(items[0].status, "inbox");
        assert!(items[0].due_date.is_some(), "「今天」→ 今日截止");

        assert_eq!(items[1].status, "done", "- [x] 已完成保留历史");
    }

    #[test]
    fn extracts_chinese_verb_sentences() {
        // 行级提取语义: 一行一项（逗号不分行）; 测试用换行分隔
        let text = "下周前提交季度报告\n尽快修复线上问题\n这是一段普通描述没有行动动词";
        let items = ActionItemExtractor::extract(text);
        assert_eq!(items.len(), 2, "两行动作句: {items:?}");
        assert!(items.iter().any(|i| i.title.contains("提交")));
        assert!(
            items
                .iter()
                .any(|i| i.title.contains("修复") && i.priority == "high"),
            "「尽快」→ high"
        );
        // 下周 → ~8 天内
        let submit = items.iter().find(|i| i.title.contains("提交")).unwrap();
        let due = submit.due_date.unwrap();
        let days = (due - Utc::now().timestamp_millis()) / 86_400_000;
        assert!((5..=10).contains(&days), "「下周」≈ 8 天: {days}");
    }

    #[test]
    fn ignores_headers_tables_and_long_lines() {
        // 真实超长行（60+ 字符）
        let long = "这".repeat(61) + "提交";
        let text = format!("# 标题里有提交二字但太短\n| 表格 | 提交 |\n> 引用里提到修复\n{long}");
        let items = ActionItemExtractor::extract(&text);
        assert!(items.is_empty(), "标题/表格/引用/超长行全部排除: {items:?}");
    }

    /// Phase 3 闭环: 提取 → 任务投影 → TodayView 聚合可见。
    #[tokio::test]
    async fn extracted_items_flow_to_today_view() {
        let bus = LayeredEventBus::new(Some(Arc::new(InMemoryEventQueue::new())));
        let p = make_projection();

        // 笔记创建（播种基础行）
        bus.publish(AppEvent::NoteCreated {
            note_id: "n1".into(),
            title: "项目周会".into(),
            content: String::new(),
        });
        bus.catch_up(&p).await.unwrap();
        assert_eq!(p.by_status("inbox").len(), 1);

        // AI 提取行动项（3 项: 1 done + 2 inbox）
        let n = ActionItemExtractor::apply_to_projection(
            "n1",
            "- [x] 发送上期周报\n- [ ] 明天提交预算表\n尽快安排评审会议",
            &p,
        )
        .await
        .unwrap();
        assert_eq!(n, 3);

        let (active, done) = p.stats();
        assert_eq!(active, 3, "基础1 + 行动2");
        assert_eq!(done, 1, "- [x] 一项完成");

        // TodayView: 「明天」截止的行动项出现在 today 窗口
        let tomorrow = Utc::now().timestamp_millis() + 86_400_000;
        let today = p.today(tomorrow);
        assert_eq!(today.len(), 1, "仅「明天提交预算表」在窗口: {today:?}");
        assert!(today[0].title.contains("预算"));
    }
}
