//! Mirror 单向 Markdown 导出（V23-I2 / T20 一期 — 差异化卖点）
//!
//! 「Obsidian 打开 mirror 目录就是一个完整可读的笔记库」——
//! 单向导出：Aurora 写出，外部只读（V22.1 §3.1 修正二 + 铁律 9/10）：
//! - 事件级落盘：单篇停止编辑约 3s 防抖，仅重写该篇（非全库）
//! - 单向明示：YAML 前置 `mirror: read-only`
//! - 附件不进 mirror（铁律 10）：相对路径引用，二进制走 Blob
//! - 反向导入属 Phase 3 且须过三压力场景 —— 本模块**永不读回**
//!
//! 路径策略：`mirror/{workspace}/{slug(title)}-{note8}.md`
//! （slug 冲突由 note_id 前 8 位消歧 — 重命名笔记不换文件名锚）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// 防抖窗口（V22.1 §3.1 修正一：约 3 秒）。
pub const MIRROR_DEBOUNCE: Duration = Duration::from_millis(3000);

/// 渲染单篇笔记为 mirror Markdown（YAML 前置 + 正文）。
///
/// 纯函数 — 同输入必同输出（全量重建幂等的根基）。
pub fn render_note_markdown(
    note_id: &str,
    title: &str,
    content: &str,
    tags: &[String],
    updated_at_rfc3339: &str,
) -> String {
    let mut yaml = String::new();
    yaml.push_str("---\n");
    yaml.push_str(&format!("aurora_id: {}\n", note_id));
    yaml.push_str(&format!("title: {}\n", yaml_escape(title)));
    yaml.push_str(&format!("updated_at: {}\n", updated_at_rfc3339));
    if tags.is_empty() {
        yaml.push_str("tags: []\n");
    } else {
        yaml.push_str("tags:\n");
        for t in tags {
            yaml.push_str(&format!("  - {}\n", yaml_escape(t)));
        }
    }
    yaml.push_str("mirror: read-only # 由 Aurora-Note 单向导出，外部修改不会被导入\n");
    yaml.push_str("---\n\n");
    format!("{}{}\n", yaml, content.trim_end())
}

/// mirror 文件相对路径（POSIX 分隔 — 跨端一致）。
pub fn mirror_rel_path(workspace: &str, title: &str, note_id: &str) -> PathBuf {
    PathBuf::from(sanitize_seg(workspace)).join(format!(
        "{}-{}.md",
        sanitize_seg(title),
        &note_id.chars().take(8).collect::<String>()
    ))
}

/// 防抖调度器（内存态 — 重启后由全量重建/未 flush 事件补齐）。
#[derive(Debug)]
pub struct MirrorScheduler {
    /// note_id → 首次待写时刻。
    pending: std::sync::Mutex<HashMap<String, Instant>>,
}

impl MirrorScheduler {
    pub fn new() -> Self {
        Self {
            pending: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// 事件到达（编辑保存即 feed；同 note 连续编辑合并窗口）。
    pub fn feed(&self, note_id: &str) {
        let mut p = self.pending.lock().unwrap();
        p.entry(note_id.to_string()).or_insert_with(Instant::now);
    }

    /// 取出已过防抖窗口的 note（调用方执行落盘后调 [`Self::complete`]）。
    pub fn flush_due(&self, now: Instant) -> Vec<String> {
        let p = self.pending.lock().unwrap();
        let due: Vec<String> = p
            .iter()
            .filter(|(_, t)| now.duration_since(**t) >= MIRROR_DEBOUNCE)
            .map(|(id, _)| id.clone())
            .collect();
        due
    }

    /// 落盘完成确认。
    pub fn complete(&self, note_id: &str) {
        self.pending.lock().unwrap().remove(note_id);
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    /// 待写集合快照（去重保序，不清空 —— 写盘后调 complete）。
    pub fn pending_ids(&self) -> Vec<String> {
        let p = self.pending.lock().unwrap();
        let mut ids: Vec<String> = p.keys().cloned().collect();
        ids.sort();
        ids
    }
}

/// mirror 根目录句柄（调度器与其配对使用 — FFI/桌面壳持有两者）。
#[derive(Clone, Debug)]
pub struct MirrorRoot(pub PathBuf);

impl MirrorRoot {
    pub fn new(root: PathBuf) -> Self {
        Self(root)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Default for MirrorScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// 落盘单篇（写穿父目录；原子替换 — temp + rename）。
pub fn write_note_to_mirror(
    mirror_root: &Path,
    rel: &Path,
    body: &str,
) -> std::io::Result<PathBuf> {
    let full = mirror_root.join(rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = full.with_extension("md.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &full)?;
    Ok(full)
}

/// 全量重建（mirror 损坏恢复 — 从事实源重导出；调用方逐篇 render + write）。
/// 返回（成功数, 失败数）。
pub fn rebuild_all(
    mirror_root: &Path,
    notes: Vec<(String, String, String, String, Vec<String>)>, // (note_id, ws, title, content, tags) + updated 需另传
    updated_of: impl Fn(&str) -> String,
) -> (usize, usize) {
    let mut ok = 0;
    let mut fail = 0;
    for (note_id, ws, title, content, tags) in notes {
        let body = render_note_markdown(&note_id, &title, &content, &tags, &updated_of(&note_id));
        let rel = mirror_rel_path(&ws, &title, &note_id);
        match write_note_to_mirror(mirror_root, &rel, &body) {
            Ok(_) => ok += 1,
            Err(_) => fail += 1,
        }
    }
    (ok, fail)
}

fn sanitize_seg(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

/// YAML 值最小转义（含特殊前导/冒号/引号时加引号）。
fn yaml_escape(s: &str) -> String {
    let needs = s.is_empty()
        || s.starts_with([
            '&', '*', '?', '|', '-', '<', '>', '=', '!', '%', '@', '#', '"', '\'', '{', '[',
        ])
        || s.contains(": ")
        || s.ends_with(':')
        || s.contains('#');
    if needs {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_yaml_structure_stable() {
        let a = render_note_markdown(
            "note-1234567890",
            "会议纪要: 周一",
            "# 议题\n正文",
            &["工作".into(), "标签#2".into()],
            "2026-09-08T00:00:00+00:00",
        );
        let b = render_note_markdown(
            "note-1234567890",
            "会议纪要: 周一",
            "# 议题\n正文",
            &["工作".into(), "标签#2".into()],
            "2026-09-08T00:00:00+00:00",
        );
        assert_eq!(a, b, "同输入必同输出（重建幂等根基）");
        assert!(a.starts_with("---\n"));
        assert!(a.contains("aurora_id: note-1234567890\n"));
        // 单向明示（铁律 9 — UI 外的文件级明示）
        assert!(a.contains("mirror: read-only"));
        // 冒号标题被引号包裹
        assert!(a.contains("title: \"会议纪要: 周一\"\n"));
        // 含 # 标签被引号包裹
        assert!(a.contains("  - \"标签#2\"\n"));
        // 正文在第二个 --- 之后
        let body = a.splitn(3, "---\n").nth(2).unwrap();
        assert!(body.contains("# 议题"));
    }

    #[test]
    fn debounce_window_behavior() {
        let s = MirrorScheduler::new();
        s.feed("n1");
        s.feed("n2");
        assert_eq!(s.pending_count(), 2);
        // 窗口内 → 不 flush
        assert!(s.flush_due(Instant::now()).is_empty());
        // 越过窗口 → flush
        let later = Instant::now() + MIRROR_DEBOUNCE + Duration::from_millis(1);
        let due = s.flush_due(later);
        assert_eq!(due.len(), 2);
        s.complete("n1");
        assert_eq!(s.pending_count(), 1);
        // 同 note 重复 feed 合并窗口（不重复计）
        s.feed("n2");
        assert_eq!(s.pending_count(), 1);
    }

    #[test]
    fn rel_path_sanitizes_and_stable() {
        let p1 = mirror_rel_path("默认空间", "我的 笔记: v1?", "abcdef1234567890");
        assert_eq!(
            p1,
            PathBuf::from("默认空间").join("我的 笔记- v1--abcdef12.md")
        );
        // 重命名标题不换锚（note8 消歧）
        let p2 = mirror_rel_path("默认空间", "改名后的标题", "abcdef1234567890");
        assert!(p2.to_string_lossy().ends_with("abcdef12.md"));
        assert_eq!(p1.extension(), Some("md".as_ref()));
    }

    #[test]
    fn write_atomic_and_rebuild_counts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let body = render_note_markdown("n1", "T", "hello", &[], "2026-09-08T00:00:00Z");
        let rel = mirror_rel_path("ws", "T", "n1");
        let full = write_note_to_mirror(root, &rel, &body).unwrap();
        assert!(full.exists());
        let (ok, fail) = rebuild_all(
            root,
            vec![("n1".into(), "ws".into(), "T".into(), "hello".into(), vec![])],
            |_| "2026-09-08T00:00:00Z".into(),
        );
        assert_eq!((ok, fail), (1, 0));
    }
}
