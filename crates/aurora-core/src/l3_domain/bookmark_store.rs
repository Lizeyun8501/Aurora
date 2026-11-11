//! A-04: 书签域（笔记/笔记本/保存的搜索——V24 R2 图 12 三类导航入口）。
//!
//! 语义区分: favorites=标记（零结构）;bookmark=**可命名、可排序、可跳转**
//! 的导航入口。KV `bm:{id}` 直存 JSON（同 event_store/task_store 模式——
//! 个人量级 O(10^2),scan_prefix+内存排序;ADR-003 日历域例外同裁决）。
//!
//! search 书签（kind=search）: `query_json` = `{ raw, rule? }`——raw=A-09
//! 高级搜索串完整保真（path:/tag:/status:/date:/短语/-排除复现面）;
//! rule=FilterRule 结构化（智能文件夹同构——卡面「复用勿另造」;由 raw
//! 可解析部分派生,不可解析则 None——双面披露,单一真相=raw）。
//!
//! 环检测（死链）: list 返回 `stale: bool`（target note/folder 不在册）
//! ——UI 显示失效态+提供移除,**禁止静默隐藏**（卡面纪律）。search 类
//! 目标恒有效（查询串非实体引用）。

use serde::{Deserialize, Serialize};

use crate::traits::kv_store::KVStore;
use crate::Error;

const BM_PREFIX: &str = "bm:";

/// 书签类别（三类——V24 R2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookmarkKind {
    /// 笔记（target=note_id）。
    Note,
    /// 笔记本/文件夹（target=folder note_id——kind=Folder 节点）。
    Folder,
    /// 保存的搜索条件（query_json 面零 target）。
    Search,
}

/// 书签实体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    /// 工作区归属（单工作区口径 default 兼容——跨工作区=停手边界外）。
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub kind: BookmarkKind,
    /// 目标（note/folder 的 note_id;search=None）。
    #[serde(default)]
    pub target_id: Option<String>,
    /// search 类: A-09 查询串+FilterRule 派生面。
    #[serde(default)]
    pub query_json: Option<String>,
    pub title: String,
    /// 排序键（0 起;reorder 全量重排——0/1/2… 紧凑无洞）。
    pub sort_order: i64,
    /// 创建时刻（epoch ms）。
    pub created_at: i64,
}

/// 列表面（死链态透出——UI 消费）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookmarkView {
    #[serde(flatten)]
    pub bookmark: Bookmark,
    /// 目标失效（note/folder 已删——search 恒 false）。
    pub stale: bool,
}

fn err_empty() -> Error {
    Error::InvalidInput("书签 id/标题不可为空".into())
}

/// 新增书签（id 调用方生成——桌面 uuid/mobile 自管;重复 id=覆盖语义
/// 同 KV 惯例——**add 幂等复写=重命名/改目标通路**,披露）。search 类
/// 必带 query_json;note/folder 必带 target_id（fail-fast）。
pub async fn add_bookmark(kv: &dyn KVStore, b: &Bookmark) -> Result<(), Error> {
    if b.id.trim().is_empty() || b.title.trim().is_empty() {
        return Err(err_empty());
    }
    match b.kind {
        BookmarkKind::Search => {
            if b.query_json.as_deref().map(str::is_empty).unwrap_or(true) {
                return Err(Error::InvalidInput("search 书签必带 query_json".into()));
            }
        }
        BookmarkKind::Note | BookmarkKind::Folder => {
            if b.target_id.as_deref().map(str::is_empty).unwrap_or(true) {
                return Err(Error::InvalidInput("note/folder 书签必带 target_id".into()));
            }
        }
    }
    // 排序键: 未指定（<0）→ 追加尾（现最大+1）
    let mut b2 = b.clone();
    if b2.sort_order < 0 {
        let all = list_raw(kv).await?;
        b2.sort_order = all.iter().map(|x| x.sort_order).max().unwrap_or(-1) + 1;
    }
    kv.set(&format!("{BM_PREFIX}{}", b2.id), &serde_json::to_vec(&b2)?)
        .await?;
    Ok(())
}

/// 全量列表（sort_order 升序;死链检测——note:{id} 在册性;损坏记录跳过
/// 并 tracing warn,不阻断列表——与 task_store 同惯例）。
pub async fn list_bookmarks(kv: &dyn KVStore) -> Result<Vec<BookmarkView>, Error> {
    let raw = list_raw(kv).await?;
    let mut views = Vec::with_capacity(raw.len());
    for b in raw {
        let stale = match b.kind {
            BookmarkKind::Search => false,
            BookmarkKind::Note | BookmarkKind::Folder => match &b.target_id {
                Some(t) => kv.get(&format!("note:{t}")).await?.is_none(),
                None => true,
            },
        };
        views.push(BookmarkView { bookmark: b, stale });
    }
    Ok(views)
}

/// 删除（幂等——不存在 Ok）。
pub async fn remove_bookmark(kv: &dyn KVStore, id: &str) -> Result<(), Error> {
    kv.delete(&format!("{BM_PREFIX}{id}")).await?;
    Ok(())
}

/// 全量重排（ids 顺序即新序——0/1/2… 紧凑重写;缺 id=保持原相对序附加尾部
/// 不丢——**禁止静默丢项**）。同时可带 title 补丁（同命令改名面——卡面
/// 「重命名」并入 reorder 语义,单命令双义,4 命令纪律披露）。
pub async fn reorder_bookmarks(
    kv: &dyn KVStore,
    ordered_ids: &[String],
    title_patch: Option<&str>,
) -> Result<(), Error> {
    let all = list_raw(kv).await?;
    let mut by_id: std::collections::HashMap<String, Bookmark> =
        all.into_iter().map(|b| (b.id.clone(), b)).collect();
    let mut out: Vec<Bookmark> = Vec::with_capacity(by_id.len());
    for (ord, id) in ordered_ids.iter().enumerate() {
        if let Some(mut b) = by_id.remove(id) {
            b.sort_order = ord as i64;
            if let Some(t) = title_patch {
                if !t.trim().is_empty() {
                    b.title = t.to_string();
                }
            }
            out.push(b);
        }
    }
    // 未提及项附加尾部（原相对序保序）
    let mut rest: Vec<Bookmark> = by_id.into_values().collect();
    rest.sort_by_key(|b| b.sort_order);
    for (i, mut b) in rest.into_iter().enumerate() {
        b.sort_order = out.len() as i64 + i as i64;
        out.push(b);
    }
    for b in out {
        kv.set(&format!("{BM_PREFIX}{}", b.id), &serde_json::to_vec(&b)?)
            .await?;
    }
    Ok(())
}

async fn list_raw(kv: &dyn KVStore) -> Result<Vec<Bookmark>, Error> {
    let mut out: Vec<Bookmark> = Vec::new();
    for (key, raw) in kv.scan_prefix(BM_PREFIX).await? {
        match serde_json::from_slice::<Bookmark>(&raw) {
            Ok(b) => out.push(b),
            Err(e) => tracing::warn!(key = %key, error = %e, "bm record skipped"),
        }
    }
    out.sort_by_key(|b| (b.sort_order, b.created_at));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use std::sync::Arc;

    fn mk(id: &str, kind: BookmarkKind, target: Option<&str>) -> Bookmark {
        Bookmark {
            id: id.into(),
            workspace_id: None,
            kind,
            target_id: target.map(String::from),
            query_json: if kind == BookmarkKind::Search {
                Some(r#"{"raw":"tag:rust date:2026-11 rust","rule":null}"#.into())
            } else {
                None
            },
            title: format!("书签-{id}"),
            sort_order: -1, // 追加尾
            created_at: 1_736_200_000_000,
        }
    }

    #[tokio::test]
    async fn three_kinds_add_list_order() {
        let kv = Arc::new(MemoryKVStore::default());
        add_bookmark(kv.as_ref(), &mk("b1", BookmarkKind::Note, Some("n1")))
            .await
            .unwrap();
        add_bookmark(kv.as_ref(), &mk("b2", BookmarkKind::Folder, Some("f1")))
            .await
            .unwrap();
        add_bookmark(kv.as_ref(), &mk("b3", BookmarkKind::Search, None))
            .await
            .unwrap();
        let list = list_bookmarks(kv.as_ref()).await.unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].bookmark.id, "b1"); // sort_order 追加序
        assert_eq!(list[2].bookmark.kind, BookmarkKind::Search);
    }

    #[tokio::test]
    async fn stale_detection_not_silent() {
        let kv = Arc::new(MemoryKVStore::default());
        add_bookmark(kv.as_ref(), &mk("b1", BookmarkKind::Note, Some("ghost")))
            .await
            .unwrap();
        let list = list_bookmarks(kv.as_ref()).await.unwrap();
        assert!(list[0].stale, "死链必须显式标态,禁静默隐藏");
    }

    #[tokio::test]
    async fn reorder_full_and_missing_kept() {
        let kv = Arc::new(MemoryKVStore::default());
        for i in 1..=3 {
            add_bookmark(
                kv.as_ref(),
                &mk(&format!("b{i}"), BookmarkKind::Note, Some("n")),
            )
            .await
            .unwrap();
        }
        // 全量重排+改名 b2
        reorder_bookmarks(
            kv.as_ref(),
            &["b3".into(), "b2".into(), "b1".into()],
            Some("改名了"),
        )
        .await
        .unwrap();
        let list = list_bookmarks(kv.as_ref()).await.unwrap();
        assert_eq!(
            list.iter()
                .map(|v| v.bookmark.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b3", "b2", "b1"]
        );
        assert_eq!(list[1].bookmark.title, "改名了");
        // 部分重排: 未提及 b1 保序附加尾部不丢
        reorder_bookmarks(kv.as_ref(), &["b2".into(), "b3".into()], None)
            .await
            .unwrap();
        let list = list_bookmarks(kv.as_ref()).await.unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[2].bookmark.id, "b1", "缺 id 必须保留,禁丢");
    }

    #[tokio::test]
    async fn remove_idempotent_and_validation() {
        let kv = Arc::new(MemoryKVStore::default());
        remove_bookmark(kv.as_ref(), "none").await.unwrap(); // 幂等
        let mut b = mk("b1", BookmarkKind::Note, None);
        assert!(
            add_bookmark(kv.as_ref(), &b).await.is_err(),
            "note 类缺 target fail-fast"
        );
        b.kind = BookmarkKind::Search;
        b.target_id = None;
        b.query_json = None;
        assert!(
            add_bookmark(kv.as_ref(), &b).await.is_err(),
            "search 类缺 query fail-fast"
        );
    }
}
