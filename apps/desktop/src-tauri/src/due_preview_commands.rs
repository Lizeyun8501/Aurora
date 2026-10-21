//! R3-1：NL 日期解析预览命令——QuickCapture 输入时实时 chip 标注。
//!
//! 复用 core `due_parse`（单一权威解析——前端零复刻）；human 为
//! tauri 侧本地时区格式化（「明天 09:00」/「周五 14:30」/「10月23日 09:00」）。

use aurora_core::l2_engines::action_extractor::due_parse;
use chrono::{Local, TimeZone};

#[derive(serde::Serialize)]
pub struct DuePreview {
    pub due_ts: i64,
    /// 命中片段字节偏移 [start, end)（core span 直传）
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
    fn of(text: &str, d: aurora_core::l2_engines::action_extractor::DueParse) -> Self {
        DuePreview {
            due_ts: d.due_ts,
            start: d.matched_start,
            end: d.matched_end,
            human: humanize(d.due_ts),
        }
    }
}

/// NL 日期片段预览（输入侧实时调用——纯函数无副作用）。
#[tauri::command]
pub async fn cmd_preview_due(text: String) -> Result<Option<DuePreview>, String> {
    // 输入侧实时——放 blocking 线程避免高频 invoke 抖动主线程（解析微秒级）
    tauri::async_runtime::spawn_blocking(move || {
        Ok(due_parse(&text).map(|d| DuePreview::of(&text, d)))
    })
    .await
    .map_err(|e| format!("preview join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_relative_days() {
        let tomorrow = (Local::now() + chrono::Duration::days(1))
            .with_hms_opt(9, 0, 0)
            .unwrap()
            .timestamp_millis();
        assert_eq!(humanize(tomorrow), "明天 09:00");
    }

    #[test]
    fn preview_of_span_matches_core() {
        let p = preview_of("明天 9 点交周报").unwrap();
        assert_eq!(&"明天 9 点交周报"[p.start..p.end], "明天 9 点");
        assert_eq!(p.human, "明天 09:00");
        assert!(preview_of("无日期散文").is_none());
    }
}
