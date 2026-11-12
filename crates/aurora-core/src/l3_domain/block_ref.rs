//! A-05（TA 2026-11-12 工单）: 块引用解析 + 面包屑路径 — 共享纯函数域。
//!
//! 设计（双端单一事实源——桌面 cmd 与移动 FFI 同源消费,禁双端各写递归）:
//! - `breadcrumb_of`: note:{id} KV 沿 parent_id 上溯（hop 上限 16/环守护/
//!   缺父即止——脏数据宽容）;输出 root→当前 有序链。
//! - `resolve_section`: 纯文本 md 切片——anchor=None 取首屏预览（首 3 非空
//!   块行）;anchor=Some(heading 文本) 取该 heading 起至下一个同级/更高级
//!   heading 止（Obsidian 章节语义）。**渲染时零全库扫描**——调用方已持
//!   目标笔记单点读取结果,本函数只做内存切片。
//!
//! 停手边界: 不做循环引用检测之外的图算法;引用只读。

use serde::Serialize;

/// 面包屑层级（双端共用 VM 字段——{id,title}[]）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Crumb {
    pub id: String,
    pub title: String,
}

/// 面包屑 hop 上限（防御脏数据环/超深树）。
const BREADCRUMB_MAX_HOPS: usize = 16;

/// 沿 parent_id 上溯面包屑链（root→当前 有序;含当前笔记）。
///
/// - 目标笔记缺失 → 空链（UI 侧隐藏面包屑,非报错——宽容语义）
/// - 单跳 title 解析失败/加密不可读 → 该级 title 退「未命名」（链不断）
/// - 环/超限 → 截断（不 panic——脏数据防御）
///
/// # Errors
/// 仅 KV 底层错误透传。
pub async fn breadcrumb_of(
    kv: &dyn crate::traits::kv_store::KVStore,
    note_id: &str,
) -> Result<Vec<Crumb>, crate::Error> {
    let mut chain: Vec<Crumb> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut cur = note_id.to_string();
    let mut hops = 0usize;
    loop {
        if !seen.insert(cur.clone()) || hops >= BREADCRUMB_MAX_HOPS {
            break; // 环守护/hop 上限
        }
        let Some(bytes) = kv.get(&format!("note:{cur}")).await? else {
            break; // 首跳缺失=目标不存在;上级缺失=脏引用,链到此为止
        };
        let json: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => break, // 元数据损坏——宽容截断
        };
        let title = json
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("未命名")
            .to_string();
        let parent = json
            .get("parent_id")
            .and_then(|p| p.as_str())
            .map(|s| s.to_string());
        chain.push(Crumb {
            id: cur.clone(),
            title,
        });
        match parent {
            Some(p) if !p.is_empty() => {
                cur = p;
                hops += 1;
            }
            _ => break,
        }
    }
    chain.reverse(); // 上溯收集=当前→root;输出 root→当前
    Ok(chain)
}

/// 引用解析状态（fail-closed 分流:锁定≠失效——锁定可恢复,失效=源已删）。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SectionResolve {
    Ok { title: String, section_md: String },
    AnchorNotFound { title: String },
}

/// anchor 判定行:前导 # 1-6 个（A-03 已保 h4-h6 往返）。
fn heading_level(line: &str) -> Option<usize> {
    let t = line.trim_start();
    if !t.starts_with('#') {
        return None;
    }
    let n = t.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&n) && t.len() > n && t.as_bytes()[n] == b' ' {
        Some(n)
    } else {
        None
    }
}

/// md 内容切片（内存纯函数——零 KV/零全库扫描）。
///
/// - anchor=None → 首屏预览:首个 heading（含）或前 3 非空块行
/// - anchor=Some → heading 文本精确匹配（trim 后）起,至下一个 ≤ 同级
///   heading 止;匹配不到 → `AnchorNotFound`
pub fn resolve_section(content_md: &str, anchor: Option<&str>) -> SectionResolve {
    // 标题取法:首个 heading 行文本 → 首个非空行 → 空串（调用方以 note
    // 元数据 title 兜底——本返回值仅为内容面回退参考）。
    let title = content_md
        .lines()
        .find_map(|l| {
            heading_level(l).map(|_| {
                l.trim_start_matches('#')
                    .trim()
                    .trim_end_matches('#')
                    .trim()
                    .to_string()
            })
        })
        .or_else(|| {
            content_md
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
        })
        .unwrap_or_default();

    let lines: Vec<&str> = content_md.lines().collect();
    let preview = |from: usize, max_blocks: usize| -> String {
        let mut out: Vec<&str> = Vec::new();
        let mut blocks = 0usize;
        let mut i = from;
        while i < lines.len() && blocks < max_blocks {
            let l = lines[i];
            if l.trim().is_empty() {
                if !out.is_empty() {
                    blocks += 1;
                }
                i += 1;
                continue;
            }
            out.push(l);
            i += 1;
        }
        out.join("\n")
    };

    match anchor {
        None => {
            let start = lines
                .iter()
                .position(|l| heading_level(l).is_some())
                .unwrap_or(0);
            SectionResolve::Ok {
                title,
                section_md: preview(start, 3),
            }
        }
        Some(a) => {
            let want = a.trim();
            let Some(start) = lines.iter().position(|l| {
                heading_level(l)
                    .map(|_| {
                        l.trim_start_matches('#')
                            .trim()
                            .trim_end_matches('#')
                            .trim()
                            == want
                    })
                    .unwrap_or(false)
            }) else {
                return SectionResolve::AnchorNotFound { title };
            };
            let level = heading_level(lines[start]).unwrap_or(1);
            let mut end = lines.len();
            for (i, l) in lines.iter().enumerate().skip(start + 1) {
                if let Some(lv) = heading_level(l) {
                    if lv <= level {
                        end = i;
                        break;
                    }
                }
            }
            SectionResolve::Ok {
                title,
                section_md: lines[start..end].join("\n"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_anchor_slicing() {
        let md = "# 笔记A\n\n## 一节\n\n内容1\n\n## 二节\n\n内容2\n\n# 结尾\n";
        match resolve_section(md, Some("一节")) {
            SectionResolve::Ok { section_md, .. } => {
                assert!(section_md.contains("## 一节"));
                assert!(section_md.contains("内容1"));
                assert!(!section_md.contains("二节"));
            }
            other => panic!("expect Ok, got {other:?}"),
        }
    }

    #[test]
    fn section_anchor_not_found() {
        let md = "# A\n\n正文\n";
        assert!(matches!(
            resolve_section(md, Some("不存在")),
            SectionResolve::AnchorNotFound { .. }
        ));
    }

    #[test]
    fn section_preview_no_anchor() {
        let md = "行一\n\n行二\n\n行三\n\n行四\n";
        match resolve_section(md, None) {
            SectionResolve::Ok { section_md, .. } => {
                assert!(section_md.contains("行一"));
                assert!(!section_md.contains("行四")); // 3 块封顶
            }
            other => panic!("expect Ok, got {other:?}"),
        }
    }
}
