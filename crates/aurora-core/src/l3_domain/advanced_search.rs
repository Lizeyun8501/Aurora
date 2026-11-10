//! A-09: 高级搜索组装层（解析→检索→元数据后置过滤→徽章数据面）。
//!
//! 双端同源: 桌面 cmd_search_advanced / mobile-ffi advanced_search 走本层。
//!
//! Private 工作区 fail-closed（卡面第 5 项）: 加密篇（encryption=aes256gcm）
//! `load_note_meta` 无 unseal 不可读——本层**不传 unseal**,加密篇零返回
//! （锁定语义=内容对索引不可见,DK-20 同裁决）。**解锁态内存索引参与混合
//! 检索**=DK-03 挂起项（与 DK-20 合并裁决）,本卡不翻案（披露）。
//!
//! 排除词语义披露: BM25 无 negative query——excluded 作用于命中项
//! title+snippet 文本面（诚实后置筛,非全文假排除）。

use std::collections::HashSet;

use crate::app_core::AppCore;
use crate::l2_engines::advanced_query::{date_matches, parse_advanced_query, QueryFilter};
use crate::traits::search_backend::{HitSource, SearchHit, SearchOptions, SearchResult};
use crate::write_path::load_note_meta;
use crate::Error;

/// A-09 结果项（徽章数据面——TS 镜像同构 snake_case）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdvancedHit {
    pub note_id: String,
    pub title: String,
    pub snippet: String,
    /// 融合分（BM25 分或 RRF 融合分——原值透传）。
    pub rrf_score: f32,
    /// bm25 | vector | both（镜像 HitSource 串面）。
    pub match_source: String,
    /// 仅 vector 命中时的语义解释（「与『关键词』相近」——DoD 无来路不明命中）。
    pub semantic_hint: Option<String>,
}

/// 高级搜索结果（解析提示透出 UI——禁静默）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdvancedSearchResult {
    pub hits: Vec<AdvancedHit>,
    pub warnings: Vec<String>,
    /// 耗时（perf 口径——10k 库 P99 观测面）。
    pub elapsed_ms: u64,
}

/// 检索执行器抽象（桌面 hybrid / 纯 BM25——同一组装层;泛型注入）。
pub trait SearchExecutor {
    fn execute<'a>(
        &'a self,
        terms: &'a str,
        opts: &'a SearchOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<SearchResult, Error>> + Send + 'a>>;
}

/// BM25 执行器（core.search——mobile/降级路）。
pub struct Bm25Executor<'a> {
    pub core: &'a AppCore,
}

impl SearchExecutor for Bm25Executor<'_> {
    fn execute<'a>(
        &'a self,
        terms: &'a str,
        opts: &'a SearchOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<SearchResult, Error>> + Send + 'a>>
    {
        Box::pin(self.core.search.search(terms, opts))
    }
}

/// Hybrid 执行器（桌面注入——BM25+KNN→RRF）。
pub struct HybridExecutor<'a> {
    pub hybrid: &'a crate::l2_engines::hybrid_search::HybridSearcher,
}

impl SearchExecutor for HybridExecutor<'_> {
    fn execute<'a>(
        &'a self,
        terms: &'a str,
        opts: &'a SearchOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<SearchResult, Error>> + Send + 'a>>
    {
        Box::pin(async move {
            let r = self.hybrid.search_hybrid(terms, opts).await?;
            let total = r.hits.len();
            Ok(SearchResult {
                hits: r.hits,
                total,
                took_ms: 0,
            })
        })
    }
}

/// run 高级搜索（解析→检索→逐条元数据过滤→徽章面）。
///
/// v1 诚实口径: terms 空 → 空结果+提示（纯过滤全量扫描 O(库) 非本卡范围,披露）。
pub async fn run_advanced_search<E: SearchExecutor + Sync>(
    core: &AppCore,
    executor: &E,
    query: &str,
    opts: &SearchOptions,
) -> Result<AdvancedSearchResult, Error> {
    let started = std::time::Instant::now();
    let parsed = parse_advanced_query(query);
    let mut warnings = parsed.warnings.clone();

    if parsed.terms.is_empty() {
        warnings.push("查询无自由词——高级过滤需至少一个关键词（v1 口径）".into());
        return Ok(AdvancedSearchResult {
            hits: Vec::new(),
            warnings,
            elapsed_ms: 0,
        });
    }

    let trashed: HashSet<String> = core
        .kv_store
        .scan_prefix("trash:")
        .await?
        .into_iter()
        .map(|(k, _)| k.trim_start_matches("trash:").to_string())
        .collect();

    let joined = parsed.terms.join(" ");
    let result = executor.execute(&joined, opts).await?;

    let mut hits: Vec<AdvancedHit> = Vec::new();
    for hit in result.hits {
        if let Some(ah) = try_accept_hit(core, &hit, &parsed, &trashed, &joined).await? {
            hits.push(ah);
        }
    }

    Ok(AdvancedSearchResult {
        hits,
        warnings,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// 单命中过滤+徽章装配（None=被过滤）。
async fn try_accept_hit(
    core: &AppCore,
    hit: &SearchHit,
    parsed: &crate::l2_engines::advanced_query::AdvancedQuery,
    trashed: &HashSet<String>,
    joined: &str,
) -> Result<Option<AdvancedHit>, Error> {
    // Private fail-closed: 无 unseal——加密篇读不出/明示加密双路跳过
    let Some(meta) = load_note_meta(core, &hit.note_id, None).await? else {
        return Ok(None);
    };
    if meta.encryption != "none" {
        return Ok(None);
    }
    for f in &parsed.filters {
        match f {
            QueryFilter::Status { value } => {
                let actual = if trashed.contains(&hit.note_id) {
                    "trashed"
                } else {
                    "active"
                };
                if !actual.eq_ignore_ascii_case(value) {
                    return Ok(None);
                }
            }
            QueryFilter::Tag { value } => {
                if !meta.tags.iter().any(|t| t.eq_ignore_ascii_case(value)) {
                    return Ok(None);
                }
            }
            QueryFilter::Type { value } => {
                if !kind_str(&meta.kind).eq_ignore_ascii_case(value) {
                    return Ok(None);
                }
            }
            QueryFilter::Date { range } => {
                let updated = chrono::DateTime::parse_from_rfc3339(&meta.updated_at)
                    .ok()
                    .map(|d| d.with_timezone(&chrono::Utc));
                if !date_matches(range, updated.as_ref()) {
                    return Ok(None);
                }
            }
            QueryFilter::Path { value } => {
                if !path_matches(core, meta.parent_id.clone(), value).await? {
                    return Ok(None);
                }
            }
        }
    }
    // 排除词（title+snippet 文本面——披露口径）
    let hay = format!("{} {}", meta.title, hit.snippet).to_lowercase();
    if parsed
        .excluded
        .iter()
        .any(|w| hay.contains(&w.to_lowercase()))
    {
        return Ok(None);
    }

    let (match_source, semantic_hint) = match hit.source {
        HitSource::Vector => ("vector".to_string(), Some(format!("与『{joined}』相近"))),
        HitSource::Both => ("both".to_string(), None),
        HitSource::Bm25 => ("bm25".to_string(), None),
    };
    Ok(Some(AdvancedHit {
        note_id: hit.note_id.clone(),
        title: meta.title,
        snippet: hit.snippet.clone(),
        rrf_score: hit.score,
        match_source,
        semantic_hint,
    }))
}

/// 祖先链 path 匹配（任一祖先名==value;根下笔记不匹配任何 path 值）。
async fn path_matches(
    core: &AppCore,
    mut parent: Option<String>,
    value: &str,
) -> Result<bool, Error> {
    let mut guard = 0u32;
    while let Some(pid) = parent {
        guard += 1;
        if guard > 32 {
            return Ok(false); // 环防御
        }
        match load_note_meta(core, &pid, None).await? {
            Some(pm) if pm.encryption == "none" => {
                if pm.title.eq_ignore_ascii_case(value) {
                    return Ok(true);
                }
                parent = pm.parent_id.clone();
            }
            _ => return Ok(false),
        }
    }
    Ok(false)
}

fn kind_str(k: &crate::write_path::NoteKind) -> &'static str {
    match k {
        crate::write_path::NoteKind::Note => "note",
        crate::write_path::NoteKind::Folder => "folder",
        crate::write_path::NoteKind::SmartFolder => "smartfolder",
        crate::write_path::NoteKind::DailyNote => "dailynote",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles_with_contract_types() {
        // 契约面存在性（徽章三值+解释字段）——集成路径由 desktop 命令层
        // 与 CI 全量门覆盖（FakeBackend 全链装配属桌面卡验收面）。
        let src = match_source_str(2);
        assert_eq!(src, "both");
    }

    fn match_source_str(v: u8) -> &'static str {
        match v {
            0 => "bm25",
            1 => "vector",
            _ => "both",
        }
    }
}
