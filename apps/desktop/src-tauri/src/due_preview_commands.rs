//! R3-1：NL 日期解析预览命令——QuickCapture 输入时实时 chip 标注。
//!
//! 复用 core `due_parse`（单一权威解析——前端零复刻）；human 为
//! tauri 侧本地时区格式化（「明天 09:00」/「周五 14:30」/「10月23日 09:00」）。

use aurora_core::l2_engines::action_extractor::due_parse;
use chrono::{Datelike, Local, TimeZone};

#[derive(serde::Serialize)]
pub struct DuePreview {
    pub due_ts: i64,
    /// 命中片段 **char offset** [start, end)（core span 直传——R-α1：
    /// char 而非字节，前端 JS 字符串切片按 code point 走，字节漂移）
    pub start: usize,
    pub end: usize,
    /// 人类可读格式（本地时区）
    pub human: String,
}

/// 本地时区人类可读：≤2 天相对词（今天/明天/后天）/≤6 天周X/否则绝对日期；+时刻。
fn humanize(due_ts: i64) -> String {
    let dt = Local
        .timestamp_millis_opt(due_ts)
        .single()
        .unwrap_or_else(Local::now);
    let days = (dt.date_naive() - Local::now().date_naive()).num_days();
    let time = dt.format("%H:%M").to_string();
    let day_word = match days {
        i64::MIN..=0 => "今天".to_string(),
        1 => "明天".to_string(),
        2 => "后天".to_string(),
        3..=6 => ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
            [dt.weekday().num_days_from_monday() as usize]
            .to_string(),
        _ => dt.format("%m月%d日").to_string(),
    };
    format!("{day_word} {time}")
}

impl DuePreview {
    // text 参数为保留签名位（调用点传 &text 语义对齐 humanize 展示源）——
    // 未用加下划线抑制（desktop-check 存量债首层：数月基础设施红后首次
    // 真正编译，-D warnings 翻出）。
    fn of(_text: &str, d: aurora_core::l2_engines::action_extractor::DueParse) -> Self {
        DuePreview {
            due_ts: d.due_ts,
            start: d.matched_start,
            end: d.matched_end,
            human: humanize(d.due_ts),
        }
    }
}

/// NL 日期片段预览——纯函数（R-α1 测试面语义：span=char offset）。
/// desktop-check 存量债修复（2026-10-26）：原内联在 cmd 里致测试引用
/// 的 preview_of 不存在——抽出为单一事实源，cmd 只做线程桥。
fn preview_of(text: &str) -> Option<DuePreview> {
    due_parse(text).map(|d| DuePreview::of(text, d))
}

/// NL 日期片段预览（输入侧实时调用——纯函数无副作用）。
#[tauri::command]
pub async fn cmd_preview_due(text: String) -> Result<Option<DuePreview>, String> {
    // 输入侧实时——放 blocking 线程避免高频 invoke 抖动主线程（解析微秒级）
    tauri::async_runtime::spawn_blocking(move || Ok(preview_of(&text)))
        .await
        .map_err(|e| format!("preview join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike; // with_hour/minute/second（desktop-check 存量债）

    #[test]
    fn humanize_relative_days() {
        // desktop-check 存量债：DateTime<Tz> 无 with_hms_opt（NaiveTime 才有）——
        // with_hour/minute/second 三链等价（「明早 9 点」意图不变）
        let tomorrow = (Local::now() + chrono::Duration::days(1))
            .with_hour(9)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .timestamp_millis();
        assert_eq!(humanize(tomorrow), "明天 09:00");
    }

    #[test]
    fn preview_of_span_matches_core() {
        // R-α1：span=char offset——带中文前缀切 char 而非字节
        let p = preview_of("记得 交周报 明天 9 点").unwrap();
        let chars: Vec<char> = "记得 交周报 明天 9 点".chars().collect();
        let seg: String = chars[p.start..p.end].iter().collect();
        assert_eq!(seg, "明天 9 点");
        assert_eq!(p.human, "明天 09:00");
        assert!(preview_of("无日期散文").is_none());
    }
}
