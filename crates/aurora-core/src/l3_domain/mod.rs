//! L3 领域服务层（Domain Service Layer）
//!
//! 包含内容编辑、知识网络、GTD效能、AI智能等P0领域服务模块，
//! 以及导入导出、素材库、系统设置、TodayView、OCR服务等P1领域服务模块。

pub mod ai_system;
pub mod content_editor;
pub mod fsrs; // V20 Phase 3: FSRS 间隔重复调度（Distill 召回）
pub mod gtd_system;
pub mod knowledge_network;

pub mod asset_library;
pub mod capture_matrix;
pub mod import_export;
pub mod ocr_service;
pub mod system_settings;
pub mod today_view;

/// V19 DEF-003 五子层架构视图（core_data / knowledge / intelligence /
/// productivity / integration），含依赖方向约束说明。
pub mod sublayers;

/// V26 M2/DK-06: 任务依赖环检测。
///
/// `deps`: task_id → 其前置任务 id 列表（依赖方向 task → depends_on）。
/// `from`: 拟新增依赖的起点（将依赖 target 的任务）。
/// `target`: 拟依赖的任务。
/// 返回 Some(环路径) 表示新增依赖会成环（拒绝创建）。
pub fn detect_dependency_cycle(
    deps: &std::collections::HashMap<String, Vec<String>>,
    from: &str,
    target: &str,
) -> Option<Vec<String>> {
    // 从 target 出发沿依赖边 DFS 找 from — 找到即成环
    let mut path = vec![target.to_string()];
    let mut visited = std::collections::HashSet::new();
    fn dfs(
        node: &str,
        goal: &str,
        deps: &std::collections::HashMap<String, Vec<String>>,
        path: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) -> bool {
        if node == goal {
            return true;
        }
        if !visited.insert(node.to_string()) {
            return false;
        }
        if let Some(nexts) = deps.get(node) {
            for next in nexts {
                path.push(next.clone());
                if dfs(next, goal, deps, path, visited) {
                    return true;
                }
                path.pop();
            }
        }
        false
    }
    if dfs(target, from, deps, &mut path, &mut visited) {
        Some(path)
    } else {
        None
    }
}

#[cfg(all(test, feature = "loro-crdt"))]
mod gtd_tests {
    use super::*;
    use crate::l1_infrastructure::note_doc::NoteTask;

    /// V26 DK-06: 7 态状态机合法迁移全覆盖。
    #[test]
    fn gtd_seven_state_transitions() {
        // 新增态合法迁移
        assert!(NoteTask::is_valid_transition("inbox", "someday"), "搁置");
        assert!(NoteTask::is_valid_transition("someday", "next"), "激活");
        assert!(
            NoteTask::is_valid_transition("someday", "scheduled"),
            "排期"
        );
        assert!(
            NoteTask::is_valid_transition("someday", "cancelled"),
            "废弃"
        );
        assert!(NoteTask::is_valid_transition("next", "cancelled"), "取消");
        assert!(
            NoteTask::is_valid_transition("waiting", "cancelled"),
            "取消"
        );
        assert!(
            NoteTask::is_valid_transition("scheduled", "cancelled"),
            "取消"
        );
        assert!(NoteTask::is_valid_transition("cancelled", "next"), "复活");
        // 非法: 直达终态互斥/越级
        assert!(
            !NoteTask::is_valid_transition("inbox", "waiting"),
            "inbox 不能直达 waiting"
        );
        assert!(
            !NoteTask::is_valid_transition("done", "cancelled"),
            "done 与 cancelled 互斥"
        );
        assert!(
            !NoteTask::is_valid_transition("cancelled", "done"),
            "cancelled 与 done 互斥"
        );
        assert!(
            !NoteTask::is_valid_transition("someday", "waiting"),
            "someday 不能直达 waiting"
        );
    }

    /// V26 DK-06: 依赖环检测 — 直环/间接环/无环。
    #[test]
    fn dependency_cycle_detection() {
        use std::collections::HashMap;
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();
        // A 依赖 B（A→B）
        deps.insert("A".into(), vec!["B".into()]);
        // 直接环: B 想依赖 A → A→B→A
        assert!(
            detect_dependency_cycle(&deps, "B", "A").is_some(),
            "直环拒绝"
        );
        // 间接环: B 依赖 C, C 依赖 A → B→C→A→B
        deps.insert("B".into(), vec!["C".into()]);
        deps.insert("C".into(), vec!["A".into()]);
        assert!(
            detect_dependency_cycle(&deps, "B", "A").is_some(),
            "间接环拒绝"
        );
        // 无环: D 想依赖 A
        assert!(
            detect_dependency_cycle(&deps, "D", "A").is_none(),
            "无环放行"
        );
        // 自依赖
        assert!(
            detect_dependency_cycle(&deps, "E", "E").is_some(),
            "自依赖拒绝"
        );
    }
}
