//! 自然语言查询解析 — V20 Phase 3（§5.4 搜索页「口语化语义查询」）
//!
//! 把「用一句话描述要找的内容」解析为结构化 [`Query`]，喂给
//! [`QueryEngine`] 统一执行。搜索页 UI 文案「支持口语化查询，如
//! 『本周未完成的任务』『关联某项目的所有笔记』」的 Rust 侧支撑。
//!
//! # 语法覆盖（本地规则 — 与 ActionItemExtractor 同哲学:
//! 确定性零依赖离线可用，AI 液化为增强）
//!
//! | 口语 | 解析 |
//! |------|------|
//! | 「本周未完成的任务」 | source=tasks, status != done, due ≤ 周末 |
//! | 「今天的任务/待办」 | source=tasks, due = 今天 |
//! | 「未完成的任务」 | source=tasks, status != done |
//! | 「紧急的任务」 | source=tasks, priority = urgent |
//! | 「标题包含 X 的笔记」 | source=notes, title contains X |
//! | 「X 的笔记」/「关于 X」 | source=notes, fulltext X |
//! | 「最近/最新 N 篇笔记」 | source=notes, sort=updated_at desc, limit N |
//! | 「引用 N 的笔记」/「链接到 N」 | source=links, target = N |
//! | 兜底 | source=notes, fulltext 整句 |
//!
//! 解析失败不报错 — 降级为全文检索（搜索的可用性优先）。
//!
//! # 路由
//! `source` 值与 QueryEngine 的执行路径对齐: tasks/links 走
//! SQLite（投影持久化表）, notes 走 Tantivy 全文。

use chrono::{Datelike, Duration, Utc, Weekday};

use crate::l2_engines::query::{Filter, Pagination, Query, Sort, SortDirection};

/// 自然语言查询解析器。
pub struct NlQueryParser;

impl NlQueryParser {
    /// 解析口语化查询为结构化 Query。
    ///
    /// 永不失败: 无法识别时降级为 notes 全文检索。
    pub fn parse(input: &str) -> Query {
        let text = input.trim();
        if text.is_empty() {
            return Self::fallback(text);
        }
        let lower = text.to_lowercase();

        // ── 任务/待办意图 ──
        let is_task_intent = lower.contains("任务")
            || lower.contains("待办")
            || lower.contains("todo")
            || lower.contains("行动项");
        if is_task_intent {
            return Self::parse_task_query(text, &lower);
        }

        // ── 链接/引用意图 ──
        if let Some(target) = Self::extract_link_target(text, &lower) {
            return Query {
                source: "links".into(),
                filter: Some(Filter::Eq {
                    field: "target_note_id".into(),
                    value: serde_json::json!(target),
                }),
                sort: Vec::new(),
                pagination: Some(Pagination { limit: 50, offset: 0 }),
                aggregation: None,
                projection: Some(vec![
                    "source_note_id".into(),
                    "target_note_id".into(),
                    "created_at".into(),
                ]),
            };
        }

        Self::parse_notes_query(text, &lower)
    }

    /// 任务意图解析（时间窗 + 状态 + 优先级组合）。
    fn parse_task_query(text: &str, lower: &str) -> Query {
        let mut filters: Vec<Filter> = Vec::new();

        // 状态
        if lower.contains("未完成") || lower.contains("未做") || lower.contains("没完成") {
            filters.push(Filter::Ne {
                field: "status".into(),
                value: serde_json::json!("done"),
            });
        } else if lower.contains("已完成") || lower.contains("完成") {
            filters.push(Filter::Eq {
                field: "status".into(),
                value: serde_json::json!("done"),
            });
        }

        // 优先级
        if lower.contains("紧急") || lower.contains("urgent") {
            filters.push(Filter::Eq {
                field: "priority".into(),
                value: serde_json::json!("urgent"),
            });
        } else if lower.contains("重要") {
            filters.push(Filter::In {
                field: "priority".into(),
                values: vec![serde_json::json!("urgent"), serde_json::json!("high")],
            });
        }

        // 时间窗（due_date 毫秒边界）
        let now = Utc::now().timestamp_millis();
        let today_end = Self::end_of_today_ms();
        if lower.contains("今天") || lower.contains("今日") {
            filters.push(Filter::Lte {
                field: "due_date".into(),
                value: serde_json::json!(today_end),
            });
        } else if lower.contains("本周") || lower.contains("这周") || lower.contains("这星期") {
            filters.push(Filter::Lte {
                field: "due_date".into(),
                value: serde_json::json!(Self::end_of_week_ms()),
            });
        } else if lower.contains("下周") {
            let week_end = Self::end_of_week_ms();
            filters.push(Filter::Gt {
                field: "due_date".into(),
                value: serde_json::json!(week_end),
            });
            filters.push(Filter::Lte {
                field: "due_date".into(),
                value: serde_json::json!(week_end + 7 * 86_400_000),
            });
        }

        // 排序: 未指定时间 → 优先级降序 + 截止升序
        let sort = vec![
            Sort {
                field: "due_date".into(),
                direction: SortDirection::Asc,
            },
            Sort {
                field: "priority".into(),
                direction: SortDirection::Desc,
            },
        ];

        Query {
            source: "tasks".into(),
            filter: if filters.len() == 1 {
                filters.pop()
            } else if filters.is_empty() {
                None
            } else {
                Some(Filter::And { filters })
            },
            sort,
            pagination: Some(Pagination { limit: 50, offset: 0 }),
            aggregation: None,
            projection: Some(vec![
                "task_id".into(),
                "note_id".into(),
                "title".into(),
                "status".into(),
                "priority".into(),
                "due_date".into(),
            ]),
            // 冗余字段抹除（text 未用 — 签名对称性）
            ..Self::fallback("")
        }
    }

    /// 笔记意图解析（最近 N / 标题包含 / 全文兜底）。
    fn parse_notes_query(text: &str, lower: &str) -> Query {
        // 最近 N 篇
        let recent = lower.contains("最近") || lower.contains("最新");
        if recent {
            let limit = Self::extract_number(lower).unwrap_or(10);
            return Query {
                source: "notes".into(),
                filter: None,
                sort: vec![Sort {
                    field: "updated_at".into(),
                    direction: SortDirection::Desc,
                }],
                pagination: Some(Pagination { limit, offset: 0 }),
                aggregation: None,
                projection: Some(vec!["note_id".into(), "title".into(), "updated_at".into()]),
            };
        }

        // 标题包含 X
        if let Some(rest) = lower.strip_prefix("标题包含") {
            let kw = Self::clean_keyword(rest);
            if !kw.is_empty() {
                return Query {
                    source: "notes".into(),
                    filter: Some(Filter::Contains {
                        field: "title".into(),
                        value: kw,
                    }),
                    sort: Vec::new(),
                    pagination: Some(Pagination { limit: 50, offset: 0 }),
                    aggregation: None,
                    projection: None,
                };
            }
        }

        // 「X 的笔记」「关于 X」
        for pat in ["的笔记", "的 笔记"] {
            if let Some(pos) = text.find(pat) {
                let kw = Self::clean_keyword(&text[..pos]);
                if !kw.is_empty() {
                    return Self::fulltext_query(&kw);
                }
            }
        }
        if let Some(rest) = text.strip_prefix("关于") {
            let kw = Self::clean_keyword(rest);
            if !kw.is_empty() {
                return Self::fulltext_query(&kw);
            }
        }

        Self::fallback(text)
    }

    /// 链接意图目标提取（「引用 X 的笔记」/「链接到 X」/「关联 X」）。
    fn extract_link_target(text: &str, lower: &str) -> Option<String> {
        for pat in ["引用", "链接到", "关联"] {
            if let Some(pos) = lower.find(pat) {
                let after = &text[pos + pat.len()..];
                let target = Self::clean_keyword(after);
                if !target.is_empty() && target.chars().count() <= 40 {
                    return Some(target);
                }
            }
        }
        None
    }

    fn fulltext_query(kw: &str) -> Query {
        Query {
            source: "notes".into(),
            filter: Some(Filter::FullText {
                query: kw.to_string(),
                fields: None,
            }),
            sort: Vec::new(),
            pagination: Some(Pagination { limit: 50, offset: 0 }),
            aggregation: None,
            projection: None,
        }
    }

    /// 兜底: 全文检索整句（搜索永不失败原则）。
    fn fallback(text: &str) -> Query {
        Self::fulltext_query(text)
    }

    fn end_of_today_ms() -> i64 {
        let now = Utc::now();
        let end = now.date() + Duration::days(1);
        end.and_hms_opt(0, 0, 0).unwrap().timestamp_millis() - 1
    }

    fn end_of_week_ms() -> i64 {
        let now = Utc::now();
        // 周日为一周末尾（中文习惯周一为首日）
        let days_to_sunday = match now.weekday() {
            Weekday::Mon => 6,
            Weekday::Tue => 5,
            Weekday::Wed => 4,
            Weekday::Thu => 3,
            Weekday::Fri => 2,
            Weekday::Sat => 1,
            Weekday::Sun => 0,
        };
        let end = now.date() + Duration::days(days_to_sunday + 1);
        end.and_hms_opt(0, 0, 0).unwrap().timestamp_millis() - 1
    }

    fn extract_number(text: &str) -> Option<usize> {
        let mut num = String::new();
        for ch in text.chars() {
            if ch.is_ascii_digit() {
                num.push(ch);
            } else if !num.is_empty() {
                break;
            }
        }
        num.parse().ok()
    }

    /// 清洗关键词（去「的」「所有」「所有…的」等虚词 + 首尾空白）。
    fn clean_keyword(s: &str) -> String {
        let mut t = s.trim().to_string();
        for suffix in ["的笔记", "的所有", "所有", "的", "笔记", "任务"] {
            if t.ends_with(suffix) {
                t.truncate(t.len() - suffix.chars().count());
                break;
            }
        }
        for prefix in ["的", "笔记"] {
            if t.starts_with(prefix) {
                t = t[prefix.chars().count()..].to_string();
            }
        }
        t.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 「本周未完成的任务」→ tasks + status!=done + due≤周末。
    #[test]
    fn parses_week_undone_tasks() {
        let q = NlQueryParser::parse("本周未完成的任务");
        assert_eq!(q.source, "tasks");
        match q.filter {
            Some(Filter::And { filters }) => {
                assert!(filters.iter().any(|f| matches!(f,
                    Filter::Ne { field, value }
                        if field == "status" && value == &serde_json::json!("done"))));
                assert!(filters.iter().any(|f| matches!(f,
                    Filter::Lte { field, .. } if field == "due_date")));
            }
            other => panic!("应为 And 组合: {other:?}"),
        }
    }

    /// 「今天的待办」→ due ≤ 今日末。
    #[test]
    fn parses_today_todos() {
        let q = NlQueryParser::parse("今天的待办");
        assert_eq!(q.source, "tasks");
        match q.filter {
            Some(f) => assert!(matches!(f,
                Filter::Lte { field, .. } if field == "due_date")),
            None => panic!("应有时间过滤"),
        }
    }

    /// 「紧急的任务」→ priority=urgent。
    #[test]
    fn parses_urgent_tasks() {
        let q = NlQueryParser::parse("紧急的任务");
        assert_eq!(q.source, "tasks");
        match q.filter {
            Some(Filter::Eq { field, value, .. }) => {
                assert_eq!(field, "priority");
                assert_eq!(value, serde_json::json!("urgent"));
            }
            _ => panic!(),
        }
    }

    /// 「最近 5 篇笔记」→ sort desc + limit 5。
    #[test]
    fn parses_recent_notes() {
        let q = NlQueryParser::parse("最近5篇笔记");
        assert_eq!(q.source, "notes");
        assert_eq!(q.pagination.unwrap().limit, 5);
        assert!(q.sort.iter().any(|s| s.field == "updated_at" && matches!(s.direction, SortDirection::Desc)));
    }

    /// 「关于架构设计」→ 全文检索。
    #[test]
    fn parses_about_fulltext() {
        let q = NlQueryParser::parse("关于架构设计");
        assert_eq!(q.source, "notes");
        match q.filter {
            Some(Filter::FullText { query, .. }) => {
                assert!(query.contains("架构设计"), "query={query}");
            }
            _ => panic!(),
        }
    }

    /// 「链接到分布式笔记」→ links + target。
    #[test]
    fn parses_link_query() {
        let q = NlQueryParser::parse("引用分布式笔记的笔记");
        // 「引用」命中 → links 意图
        assert_eq!(q.source, "links");
        match q.filter {
            Some(Filter::Eq { field, value, .. }) => {
                assert_eq!(field, "target_note_id");
                assert!(value.as_str().unwrap_or("").contains("分布式"));
            }
            _ => panic!(),
        }
    }

    /// 无法识别 → notes 全文兜底（永不失败）。
    #[test]
    fn falls_back_to_fulltext() {
        let q = NlQueryParser::parse("随便一段完全看不懂的话");
        assert_eq!(q.source, "notes");
        assert!(matches!(q.filter, Some(Filter::FullText { .. })));
    }

    /// 空输入 → 兜底空查询（不 panic）。
    #[test]
    fn empty_input_ok() {
        let q = NlQueryParser::parse("");
        assert_eq!(q.source, "notes");
    }
}
