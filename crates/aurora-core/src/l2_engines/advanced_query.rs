//! A-09: 高级搜索查询解析器（纯函数——BM25/hybrid 消费前的语法面）。
//!
//! 语法（5 类运算符+短语+排除词）:
//! - `path:子树` `tag:标签` `status:active|trashed|encrypted` `type:kind`
//!   `date:2026-11 | date:2026-11-01..11-30 | date:>2026-10-01 | date:<2026-11-01`
//! - 引号短语: `"知识 管理"` 整词共现
//! - 排除词: `-词`（结果不含）
//!
//! 解析纪律（卡面）: **失败回落纯文本并给出提示,不静默忽略运算符**——
//! 未知运算符（如 `title:xxx`）/值缺失/date 形态非法 → warnings 收口+
//! 该 token 原样并入 terms（搜索仍可执行,提示透出 UI）。
//!
//! 语义映射（按真实通路,报告披露）: status 无笔记级状态域——映射
//! active（正常）/trashed（软删）/encrypted（加密面）;type=NoteKind;
//! date 作用于 updated_at。

use chrono::{DateTime, NaiveDate, Utc};

/// 值域过滤条件（后置——命中集上做元数据过滤）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueryFilter {
    /// 文件夹/路径子树（name 前缀匹配——tree 投影消费）。
    Path { value: String },
    /// 标签（多 tag: 为 AND）。
    Tag { value: String },
    /// 状态（active/trashed/encrypted——映射披露）。
    Status { value: String },
    /// 笔记类型（NoteKind 串）。
    Type { value: String },
    /// updated_at 条件。
    Date { range: DateRange },
}

/// date: 值形态。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateRange {
    /// 单日（YYYY-MM-DD）。
    On { date: NaiveDate },
    /// 单月（YYYY-MM）/单年（YYYY）。
    Within { prefix: String },
    /// 闭区间 a..b（含端点;短形态可混:2026-11-01..11-30）。
    Between { from: NaiveDate, to: NaiveDate },
    /// > 值（严格晚于当日 00:00）。
    After { date: NaiveDate },
    /// < 值（严格早于次日 00:00）。
    Before { date: NaiveDate },
}

/// 解析产物: 自由词+排除词+过滤+提示（warnings 空=全语法干净解析）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AdvancedQuery {
    /// 自由词/引号短语（AND 语义交给底层 BM25/hybrid）。
    pub terms: Vec<String>,
    /// 排除词（-前缀;NOT 语义,命中集后置剔除）。
    pub excluded: Vec<String>,
    /// 运算符过滤（同 kind 多值=AND;跨 kind=AND）。
    pub filters: Vec<QueryFilter>,
    /// 解析提示（未知运算符/非法 date 等——UI 透出,禁静默）。
    pub warnings: Vec<String>,
}

/// date token → DateRange（None=非法形态,由调用方 warning）。
fn parse_date_range(raw: &str) -> Option<DateRange> {
    let s = raw.trim();
    if let Some(v) = s.strip_prefix('>') {
        return NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d")
            .ok()
            .map(|d| DateRange::After { date: d });
    }
    if let Some(v) = s.strip_prefix('<') {
        return NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d")
            .ok()
            .map(|d| DateRange::Before { date: d });
    }
    if let Some((a, b)) = s.split_once("..") {
        let parse_short = |part: &str, year: i32| -> Option<NaiveDate> {
            let p = part.trim();
            if let Ok(d) = NaiveDate::parse_from_str(p, "%Y-%m-%d") {
                return Some(d);
            }
            // 短形态补年（..11-30 → 同年）
            let with_year = format!("{year}-{p}");
            NaiveDate::parse_from_str(&with_year, "%Y-%m-%d").ok()
        };
        let year = chrono::Utc::now()
            .format("%Y")
            .to_string()
            .parse::<i32>()
            .unwrap_or(2026);
        let from = parse_short(a, year)?;
        let to = parse_short(b, year)?;
        return Some(DateRange::Between { from, to });
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(DateRange::On { date: d });
    }
    if NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").is_ok() {
        return Some(DateRange::Within {
            prefix: s.to_string(),
        });
    }
    if NaiveDate::parse_from_str(&format!("{s}-01-01"), "%Y-%m-%d").is_ok() {
        return Some(DateRange::Within {
            prefix: s.to_string(),
        });
    }
    None
}

const KNOWN_OPS: &[&str] = &["path", "tag", "status", "type", "date"];

/// 解析入口（纯函数,零 IO——双端同源:桌面 cmd_search_advanced /
/// mobile-ffi advanced_search 同一实现）。
pub fn parse_advanced_query(input: &str) -> AdvancedQuery {
    let mut q = AdvancedQuery::default();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;
    let mut plain = String::new();

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // 引号短语（未闭合→整体并入 terms+warning）
        if c == '"' {
            let mut j = i + 1;
            let mut phrase = String::new();
            while j < chars.len() && chars[j] != '"' {
                phrase.push(chars[j]);
                j += 1;
            }
            if j < chars.len() {
                q.terms.push(phrase);
                i = j + 1;
            } else {
                q.warnings.push("引号短语未闭合——已按普通词处理".into());
                if !phrase.is_empty() {
                    q.terms.push(phrase);
                }
                i = j;
            }
            continue;
        }
        // 排除词 -xxx（-后须非空;裸 - 视普通字符）
        if c == '-' && i + 1 < chars.len() && !chars[i + 1].is_whitespace() {
            let mut j = i + 1;
            let mut word = String::new();
            while j < chars.len() && !chars[j].is_whitespace() {
                word.push(chars[j]);
                j += 1;
            }
            q.excluded.push(word);
            i = j;
            continue;
        }
        // token（可能带运算符前缀）
        let mut j = i;
        let mut token = String::new();
        while j < chars.len() && !chars[j].is_whitespace() {
            token.push(chars[j]);
            j += 1;
        }
        // 运算符判定: op:value
        let lower = token.to_lowercase();
        let mut matched_op: Option<&str> = None;
        for op in KNOWN_OPS {
            if let Some(rest) = lower.strip_prefix(&format!("{op}:")) {
                let _ = rest;
                matched_op = Some(op);
                break;
            }
        }
        if let Some(op) = matched_op {
            let raw_value: String = token.chars().skip(op.len() + 1).collect();
            if raw_value.is_empty() {
                q.warnings
                    .push(format!("运算符 {op}: 缺值——已忽略该 token"));
            } else {
                match op {
                    "date" => match parse_date_range(&raw_value) {
                        Some(r) => q.filters.push(QueryFilter::Date { range: r }),
                        None => {
                            q.warnings.push(format!(
                                "date:{raw_value} 形态非法（支持 2026-11 / 2026-11-01..11-30 / >2026-10-01）——已按普通词搜索"
                            ));
                            q.terms.push(token.clone());
                        }
                    },
                    "path" => q.filters.push(QueryFilter::Path { value: raw_value }),
                    "tag" => q.filters.push(QueryFilter::Tag { value: raw_value }),
                    "type" => q.filters.push(QueryFilter::Type { value: raw_value }),
                    "status" => {
                        let v = raw_value.to_lowercase();
                        if ["active", "trashed", "encrypted"].contains(&v.as_str()) {
                            q.filters.push(QueryFilter::Status { value: v });
                        } else {
                            q.warnings.push(format!(
                                "status:{v} 值未知（active/trashed/encrypted）——已按普通词搜索"
                            ));
                            q.terms.push(token.clone());
                        }
                    }
                    _ => unreachable!(),
                }
            }
        } else if lower.contains(':') && !token.contains('"') {
            // 疑似未知运算符（title: 之类）——回落+提示
            q.warnings.push(format!(
                "「{token}」不是可识别的运算符（path/tag/status/type/date）——已按普通词搜索"
            ));
            q.terms.push(token.clone());
        } else {
            plain.push_str(&token);
            plain.push(' ');
        }
        i = j;
    }
    if !plain.trim().is_empty() {
        // 自由词按空白拆（短语已单独收集）
        for w in plain.split_whitespace() {
            q.terms.push(w.to_string());
        }
    }
    q
}

/// date 过滤求值（updated_at vs DateRange——none updated 恒 false,诚实）。
pub fn date_matches(range: &DateRange, updated: Option<&DateTime<Utc>>) -> bool {
    let Some(u) = updated else { return false };
    let d = u.date_naive();
    let s = u.to_rfc3339();
    match range {
        DateRange::On { date } => d == *date,
        DateRange::Within { prefix } => s.starts_with(prefix.as_str()),
        DateRange::Between { from, to } => d >= *from && d <= *to,
        DateRange::After { date } => d > *date,
        DateRange::Before { date } => d < *date,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_terms_only() {
        let q = parse_advanced_query("rust 系统 笔记");
        assert_eq!(q.terms, vec!["rust", "系统", "笔记"]);
        assert!(q.filters.is_empty() && q.warnings.is_empty());
    }

    #[test]
    fn five_ops_combinable() {
        let q =
            parse_advanced_query("path:日记 tag:rust status:active type:daily date:2026-11 关键词");
        assert_eq!(q.filters.len(), 5);
        assert_eq!(q.terms, vec!["关键词"]);
        assert!(q.warnings.is_empty());
    }

    #[test]
    fn quoted_phrase_intact() {
        let q = parse_advanced_query("\"知识 管理\" 系统");
        assert!(q.terms.contains(&"知识 管理".to_string()));
        assert!(q.terms.contains(&"系统".to_string()));
    }

    #[test]
    fn unclosed_quote_warns() {
        let q = parse_advanced_query("\"未闭合 词");
        assert!(!q.warnings.is_empty());
        assert!(q.terms.contains(&"未闭合 词".to_string()));
    }

    #[test]
    fn excluded_words() {
        let q = parse_advanced_query("搜索 -噪音 -广告");
        assert_eq!(q.excluded, vec!["噪音", "广告"]);
    }

    #[test]
    fn unknown_op_falls_back_with_hint() {
        let q = parse_advanced_query("title:abc rust");
        assert!(q.warnings.iter().any(|w| w.contains("title")));
        assert!(q.terms.contains(&"title:abc".to_string()));
        assert!(q.terms.contains(&"rust".to_string()));
    }

    #[test]
    fn date_forms() {
        let q = parse_advanced_query("date:2026-11");
        assert!(matches!(
            q.filters[0],
            QueryFilter::Date {
                range: DateRange::Within { .. }
            }
        ));
        let q = parse_advanced_query("date:>2026-10-01");
        assert!(matches!(
            q.filters[0],
            QueryFilter::Date {
                range: DateRange::After { .. }
            }
        ));
        let q = parse_advanced_query("date:2026-11-01..11-30");
        assert!(matches!(
            q.filters[0],
            QueryFilter::Date {
                range: DateRange::Between { .. }
            }
        ));
        let q = parse_advanced_query("date:开天辟地");
        assert!(!q.warnings.is_empty(), "非法 date 必须提示,不静默");
    }

    #[test]
    fn empty_value_op_warns() {
        let q = parse_advanced_query("tag: rust");
        assert!(q.warnings.iter().any(|w| w.contains("缺值")));
    }

    #[test]
    fn date_matching_semantics() {
        let u = DateTime::parse_from_rfc3339("2026-11-10T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(date_matches(
            &DateRange::On {
                date: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
            },
            Some(&u)
        ));
        assert!(date_matches(
            &DateRange::Within {
                prefix: "2026-11".into()
            },
            Some(&u)
        ));
        assert!(!date_matches(
            &DateRange::Before {
                date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
            },
            Some(&u)
        ));
        assert!(date_matches(
            &DateRange::After {
                date: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
            },
            Some(&u)
        ));
        assert!(!date_matches(
            &DateRange::On {
                date: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
            },
            None
        ));
    }
}
