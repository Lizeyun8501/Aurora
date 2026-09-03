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

use chrono::{Duration, Utc};

use crate::event_bus::layered::AppEvent;
use crate::event_bus::projection::Projection;
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
    "提交", "发送", "回复", "整理", "完成", "确认", "预约", "购买", "预定",
    "联系", "电话", "汇报", "写", "修改", "审核", "复核", "部署", "上线",
    "修复", "测试", "发布", "准备", "安排", "报名", "缴费", "续费", "归还",
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

/// 相对期限词 → 截止时间（从 now 起算）。
fn due_date_of(text: &str) -> Option<i64> {
    let now = Utc::now();
    let days = if text.contains("今天") || text.contains("今日") {
        Some(0)
    } else if text.contains("明天") || text.contains("明日") {
        Some(1)
    } else if text.contains("后天") {
        Some(2)
    } else if text.contains("本周") || text.contains("这周") {
        Some(5)
    } else if text.contains("下周") {
        Some(8)
    } else if text.contains("月底") {
        Some(20)
    } else if text.contains("周五") || text.contains("周5") {
        Some(3)
    } else if text.contains("周一") {
        Some(6)
    } else if text.contains("下周内") {
        Some(7)
    } else {
        None
    }?;
    Some((now + Duration::days(days)).timestamp_millis())
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

    /// 提取并应用到任务投影（发 TaskStatusChanged 前置: 直接播种行,
    /// 幂等 — task_id 稳定派生自 note_id + 行号）。
    pub async fn apply_to_projection(
        note_id: &str,
        text: &str,
        projection: &TaskProjection,
    ) -> Result<usize, crate::Error> {
        let items = Self::extract(text);
        let n = items.len();
        for (idx, item) in items.into_iter().enumerate() {
            // 稳定 task_id（幂等重放）
            projection.seed_row(
                &format!("ai:{note_id}:{idx}"),
                note_id,
                &item.title,
                item.status,
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

    use crate::event_bus::layered::InMemoryEventQueue;
    use crate::event_bus::layered::LayeredEventBus;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use crate::l2_engines::task_projection::TaskProjection;

    fn make_projection() -> TaskProjection {
        TaskProjection::new(Arc::new(MemoryKVStore::default()), Box::new(Vec::new))
    }

    #[test]
    fn extracts_markdown_tasks_with_metadata() {
        let text = "# 会议纪要\n- [ ] 整理今天的会议记录\n- [x] 发送周报\n普通段落";
        let items = ActionItemExtractor::extract(&text);
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
        let items = ActionItemExtractor::extract(&text);
        assert_eq!(items.len(), 2, "两行动作句: {items:?}");
        assert!(items.iter().any(|i| i.title.contains("提交")));
        assert!(items.iter().any(|i| i.title.contains("修复") && i.priority == "high"), "「尽快」→ high");
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
