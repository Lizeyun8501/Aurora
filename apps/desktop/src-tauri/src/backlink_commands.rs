//! DK-UI-S2：反链面板数据层——S1 `backlinks_of` 读取侧首个消费方。
//!
//! 领地：desktop/tauri（新文件）；读链复用 unwrap_note_bytes（pub(crate)，
//! aab8997 先例）。锁定态纪律：反链项只出 note_id+引用方标题（标题读
//! meta 失败=锁定篇 → title=None，UI 显示 note_id——不触碰内容/无解密
//! 路径）；当前笔记锁定（unwrap fail-closed）→ 空集降级，UI 空态占位。

use crate::{get_core, get_vault, unwrap_note_bytes};
use aurora_core::links::backlinks_of;
use serde::Serialize;

/// 反链项 — title=None 表示引用方为锁定篇（标题不可读，仅知有引用）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklinkItem {
    pub note_id: String,
    pub title: Option<String>,
}

/// 反向链接列表（DK-UI-S2）。
///
/// note_id → 当前笔记标题（锁定态 unwrap 失败 → 空集降级，不阻断面板）
/// → backlinks_of（S1 原语，倒排 b64url 键内聚）→ 逐引用方读 meta 标题
/// （读不出 = 锁定篇 → title=None）。
///
/// # Errors
/// 应用未初始化 / KV 读失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_backlinks_of(note_id: String) -> Result<Vec<BacklinkItem>, String> {
    let core = get_core()?;
    let vault = get_vault()?;

    // 当前笔记标题（反链查询目标）——锁定篇 fail-closed → 空集降级
    let data = match core
        .kv_store
        .get(&format!("note:{note_id}"))
        .await
        .map_err(|e| e.to_string())?
    {
        Some(d) => d,
        None => return Ok(Vec::new()),
    };
    let title = match unwrap_note_bytes(&core, &vault, &data) {
        Ok(json) => json
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("untitled")
            .to_string(),
        Err(_) => return Ok(Vec::new()), // 锁定/不可读：空态降级（无解密路径）
    };

    // 倒排读（S1 原语）——失败同样降级空集（面板非阻断纪律）
    let ids = match backlinks_of(core.kv_store.as_ref(), &title).await {
        Ok(set) => set,
        Err(e) => {
            tracing::warn!(note_id = %note_id, error = %e, "backlinks query failed; degrade to empty");
            return Ok(Vec::new());
        }
    };

    // 引用方标题补全：锁定篇 title=None（UI 回退显示 note_id）
    let mut items = Vec::with_capacity(ids.len());
    for id in ids {
        let title = match core
            .kv_store
            .get(&format!("note:{id}"))
            .await
            .map_err(|e| e.to_string())?
        {
            Some(d) => match unwrap_note_bytes(&core, &vault, &d) {
                Ok(json) => json
                    .get("title")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string()),
                Err(_) => None,
            },
            None => None,
        };
        items.push(BacklinkItem { note_id: id, title });
    }
    Ok(items)
}

// ===== 链接图谱数据层（DK-链接图谱原型——2.1 尾巴之二）=====

/// 图谱节点 — id=None 表示虚节点（悬挂目标/锁定篇目标——标题可读但无
/// 可达笔记），UI 虚显示。
#[derive(Serialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: Option<String>,
    pub title: String,
    pub degree: usize,
}

/// 图谱边 — dst 以标题为锚（S1 出链集语义），dst_title 反查存活笔记 id。
#[derive(Serialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub src_id: String,
    pub dst_title: String,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LinksGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub truncated: bool,
}

/// 图谱聚合纯函数（可测——CI 桌面面）：trash 排除 → 边收集 → 节点去重
/// （src 按 id、悬挂 dst 按标题）→ 度数（出+入）→ degree Top `max_nodes`
/// 裁剪（被裁节点两端边删除，truncated 置位）。
pub fn build_links_graph(
    note_pairs: &[(String, Option<String>)],
    trash_ids: &std::collections::HashSet<String>,
    out_pairs: &[(String, Vec<String>)],
    max_nodes: usize,
) -> LinksGraph {
    // title → (id, title) 存活映射（trash 排除；锁定篇 title=None 不入映射）
    let mut title_map: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut alive: Vec<(String, Option<String>)> = Vec::new();
    for (id, title) in note_pairs {
        if trash_ids.contains(id) {
            continue;
        }
        if let Some(t) = title {
            title_map.entry(t.as_str()).or_insert(id);
        }
        alive.push((id.clone(), title.clone()));
    }

    // 边收集：src 存活且 out 非空标题
    let mut edges: Vec<GraphEdge> = Vec::new();
    for (src_id, outs) in out_pairs {
        if trash_ids.contains(src_id) {
            continue;
        }
        for t in outs {
            if !t.is_empty() {
                edges.push(GraphEdge {
                    src_id: src_id.clone(),
                    dst_title: t.clone(),
                });
            }
        }
    }

    // 节点集：存活 src（id 在）∪ 被引 dst（存活反查成功→复用该节点；失败→悬挂虚节点）
    // id → node 索引；悬挂 title → node 索引
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut idx_by_id: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut idx_by_title: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for (id, title) in &alive {
        let n = nodes.len();
        nodes.push(GraphNode {
            id: Some(id.clone()),
            title: title.clone().unwrap_or_default(),
            degree: 0,
        });
        idx_by_id.insert(id.clone(), n);
        if let Some(t) = title {
            idx_by_title.entry(t.clone()).or_insert(n);
        }
    }

    // 悬挂/锁定目标虚节点（dst_title 反查失败）
    for e in &edges {
        if !idx_by_title.contains_key(&e.dst_title) {
            let n = nodes.len();
            nodes.push(GraphNode {
                id: None,
                title: e.dst_title.clone(),
                degree: 0,
            });
            idx_by_title.insert(e.dst_title.clone(), n);
        }
    }

    // 度数：出（有 out 集的存活篇）+入（dst 经 title 反查；虚节点只计入边）
    let mut deg: Vec<usize> = vec![0; nodes.len()];
    for e in &edges {
        if let Some(&i) = idx_by_title.get(&e.dst_title) {
            deg[i] += 1;
        }
    }
    let srcs_with_outs: std::collections::HashSet<&str> =
        out_pairs.iter().map(|(id, _)| id.as_str()).collect();
    for (id, _) in &alive {
        if srcs_with_outs.contains(id.as_str()) {
            if let Some(&i) = idx_by_id.get(id) {
                deg[i] += 1;
            }
        }
    }
    for (i, d) in deg.iter().enumerate() {
        nodes[i].degree = *d;
    }

    // 裁剪：degree Top max_nodes；被裁节点两端边删除
    let mut truncated = false;
    if nodes.len() > max_nodes {
        truncated = true;
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(nodes[i].degree));
        order.truncate(max_nodes);
        let keep: std::collections::HashSet<usize> = order.into_iter().collect();
        nodes = nodes
            .into_iter()
            .enumerate()
            .filter(|(i, _)| keep.contains(i))
            .map(|(_, n)| n)
            .collect();
        // 重映射：删边（src 被裁或 dst 虚节点被裁）
        let alive_ids: std::collections::HashSet<&str> =
            nodes.iter().filter_map(|n| n.id.as_deref()).collect();
        let kept_titles: std::collections::HashSet<&str> =
            nodes.iter().map(|n| n.title.as_str()).collect();
        edges.retain(|e| {
            alive_ids.contains(e.src_id.as_str()) && kept_titles.contains(e.dst_title.as_str())
        });
    }

    LinksGraph {
        nodes,
        edges,
        truncated,
    }
}

/// 全库链接图谱（DK-链接图谱原型）——links:out 扫描+标题补全聚合。
/// 锁定篇参与（title=None 显示短 id）；悬挂目标虚显示；trash 排除；
/// 80 节点 degree 裁剪护栏（设计稿 §3）。
///
/// # Errors
/// 应用未初始化 / KV 读失败（字符串化透传 UI）。
#[tauri::command]
pub async fn cmd_links_graph() -> Result<LinksGraph, String> {
    let core = get_core()?;
    let vault = get_vault()?;

    let trash_ids: std::collections::HashSet<String> = core
        .kv_store
        .scan_prefix("trash:")
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(k, _)| k.trim_start_matches("trash:").to_string())
        .collect();

    let mut note_pairs: Vec<(String, Option<String>)> = Vec::new();
    for (k, bytes) in core
        .kv_store
        .scan_prefix("note:")
        .await
        .map_err(|e| e.to_string())?
    {
        let Some(id) = k.strip_prefix("note:").map(|s| s.to_string()) else {
            continue;
        };
        let title = match unwrap_note_bytes(&core, &vault, &bytes) {
            Ok(json) => json
                .get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string()),
            Err(_) => None, // 锁定篇参与图谱（title=None 显示短 id）
        };
        note_pairs.push((id, title));
    }

    let out_pairs: Vec<(String, Vec<String>)> = core
        .kv_store
        .scan_prefix(aurora_core::links::LINKS_OUT_PREFIX)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(k, bytes)| {
            let id = k
                .strip_prefix(aurora_core::links::LINKS_OUT_PREFIX)
                .map(|s| s.to_string())?;
            serde_json::from_slice::<Vec<String>>(&bytes)
                .ok()
                .map(|outs| (id, outs))
        })
        .collect();

    Ok(build_links_graph(&note_pairs, &trash_ids, &out_pairs, 80))
}

#[cfg(test)]
mod links_graph_tests {
    use super::*;

    fn ids(v: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.map(|s| s.to_string())))
            .collect()
    }

    #[test]
    fn graph_basic_degree_and_dedup() {
        let notes = ids(&[("a", Some("甲")), ("b", Some("乙")), ("c", Some("丙"))]);
        let outs = vec![
            ("a".to_string(), vec!["乙".to_string(), "丙".to_string()]),
            ("b".to_string(), vec!["甲".to_string()]),
        ];
        let g = build_links_graph(&notes, &Default::default(), &outs, 80);
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.edges.len(), 3);
        let deg = |t: &str| g.nodes.iter().find(|n| n.title == t).unwrap().degree;
        assert_eq!(deg("甲"), 2, "出1+入1"); // a→乙,丙; b→甲: 甲出1入1
        assert_eq!(deg("乙"), 2, "出1+入1");
        assert_eq!(deg("丙"), 1, "仅入1");
        assert!(!g.truncated);
    }

    #[test]
    fn graph_trash_excluded() {
        let notes = ids(&[("a", Some("甲")), ("b", Some("乙"))]);
        let mut trash = std::collections::HashSet::new();
        trash.insert("b".to_string());
        let outs = vec![
            ("a".to_string(), vec!["乙".to_string()]),
            ("b".to_string(), vec!["甲".to_string()]),
        ];
        let g = build_links_graph(&notes, &trash, &outs, 80);
        assert!(g.nodes.iter().all(|n| n.title != "乙"), "trash 篇不入图");
        assert_eq!(g.edges.len(), 0, "trash src 的边全删");
        // 甲节点仍在（存活），指向乙的边成悬挂
        assert_eq!(g.nodes.len(), 2, "甲+悬挂虚节点乙");
    }

    #[test]
    fn graph_hanging_virtual_node() {
        let notes = ids(&[("a", Some("甲"))]);
        let outs = vec![("a".to_string(), vec!["不存在".to_string()])];
        let g = build_links_graph(&notes, &Default::default(), &outs, 80);
        assert_eq!(g.nodes.len(), 2);
        let virt = g.nodes.iter().find(|n| n.title == "不存在").unwrap();
        assert_eq!(virt.id, None, "悬挂目标 id=None 虚显示");
        assert_eq!(g.edges.len(), 1);
    }

    #[test]
    fn graph_locked_src_participates() {
        // 锁定篇（title=None）out 集照常入图（KV 明文），节点显示短 id
        let notes = ids(&[("a", Some("甲")), ("locked", None)]);
        let outs = vec![("locked".to_string(), vec!["甲".to_string()])];
        let g = build_links_graph(&notes, &Default::default(), &outs, 80);
        let lk = g
            .nodes
            .iter()
            .find(|n| n.id.as_deref() == Some("locked"))
            .unwrap();
        assert_eq!(lk.title, "", "锁定篇 title 空（UI 回退短 id）");
        assert_eq!(g.edges.len(), 1);
    }

    #[test]
    fn graph_truncate_top_degree() {
        // 度差明确：甲=出2+入1=3 最高；乙=出1+入1=2；丙=入1=1 最低
        let notes = ids(&[("a", Some("甲")), ("b", Some("乙")), ("c", Some("丙"))]);
        let outs = vec![
            ("a".to_string(), vec!["乙".to_string(), "丙".to_string()]),
            ("b".to_string(), vec!["甲".to_string()]),
        ];
        let g = build_links_graph(&notes, &Default::default(), &outs, 2);
        assert!(g.truncated);
        assert_eq!(g.nodes.len(), 2, "裁到 Top2");
        assert!(g.nodes.iter().any(|n| n.title == "甲"), "最高度保留");
        assert!(g.nodes.iter().any(|n| n.title == "乙"), "次高度保留");
        assert!(g.nodes.iter().all(|n| n.title != "丙"), "最低度被裁");
        assert_eq!(g.edges.len(), 2, "指向丙的边删，其余存活");
    }
}
