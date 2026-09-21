//! 迁移向导内核 — 预扫（plan）/ 会话清单防重（manifest）/ 进度推送 /
//! 选择性导入（only）。
//!
//! 设计约束（与 Alpha 裁决对齐，`issues/wf/bravo-request-import-*
//! desktop-wiring.md` 裁决问题 3）：
//! - 会话清单落地 `<data_dir>/imports/<ts>/manifest.json`，新增文件、
//!   不改既有路径语义（sidecar / 占位链接均不受影响）；
//! - 防重键 = `(source 相对路径, content_hash)`——同路径同内容一律跳过，
//!   内容变更（哪怕同路径）视为新内容重新导入；
//! - 进度经 `tokio::sync::mpsc::UnboundedSender<ProgressEvent>` 推送，
//!   桌面 command 层可直接桥接 Tauri channel。

use std::io::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::enex;
use crate::markdown;
use crate::report::ImportError;

/// 导入源类型（UI 展示用）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    /// Markdown 目录。
    #[default]
    Markdown,
    /// ENEX 单文件（多笔记）。
    Enex,
    /// OPML 单文件（多笔记）。
    Opml,
}

/// 预扫条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    /// 相对源标识（markdown：相对根路径；enex/opml：文件名）。
    pub source: String,
    /// 预读标题（md：frontmatter/h1/stem；enex：note title；opml：outline text）。
    pub title: String,
    /// 内容指纹（sha256 前 16 hex）——manifest 防重键。
    pub content_hash: String,
    /// 原始字节数。
    pub bytes: u64,
    /// 附带资源数（enex resource 计数；md/opml 恒 0）。
    pub resources: usize,
}

/// 预扫计划（纯只读，不写库）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPlan {
    /// 源类型。
    pub kind: ImportKind,
    /// 条目（markdown 按文件名排序，与导入顺序一致）。
    pub items: Vec<PlanItem>,
    /// 原始字节合计。
    pub total_bytes: u64,
}

/// 进度事件（1-based）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// 当前序号（从 1 起）。
    pub current: u32,
    /// 本会话应处理总数（`only` 过滤后）。
    pub total: u32,
    /// 当前条目的 source 标识。
    pub source: String,
}

/// manifest 条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// source 标识（markdown 相对路径 / enex `<file>#<标题>`）。
    pub source: String,
    /// 内容指纹（sha256 前 16 hex）。
    pub content_hash: String,
    /// 导入产物 note_id。
    pub note_id: String,
    /// 笔记标题。
    pub title: String,
}

/// 导入会话清单（防重）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportManifest {
    /// 已导入条目。
    pub entries: Vec<ManifestEntry>,
}

impl ImportManifest {
    /// 从 `manifest_dir/manifest.json` 读取；文件不存在/损坏 → 空清单。
    pub fn load(manifest_dir: &Path) -> Self {
        let path = manifest_dir.join("manifest.json");
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// 原子落盘（temp + rename）到 `manifest_dir/manifest.json`。
    ///
    /// # Errors
    /// 目录创建 / 临时文件写入 / rename 失败。
    pub fn save(&self, manifest_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(manifest_dir)?;
        let tmp = manifest_dir.join("manifest.json.tmp");
        let dst = manifest_dir.join("manifest.json");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(
                serde_json::to_string_pretty(self)
                    .unwrap_or_default()
                    .as_bytes(),
            )?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &dst)
    }

    /// 防重判定：同 source 且同内容指纹。
    pub fn contains(&self, source: &str, content_hash: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.source == source && e.content_hash == content_hash)
    }

    /// 记录一条已导入。
    pub fn record(&mut self, entry: ManifestEntry) {
        self.entries.push(entry);
    }
}

/// 内容指纹：sha256 前 16 hex 字符。
pub fn content_hash_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

/// 预扫 Markdown 目录（与 [`crate::import_markdown_dir`] 同一扫描语义：
/// 排序、隐藏判定、扩展名过滤、深度上限一致）。
///
/// # Errors
/// 根目录不存在 / 遍历失败。
pub fn plan_markdown_dir(
    root: &Path,
    options: &crate::ImportOptions,
) -> Result<ImportPlan, ImportError> {
    if !root.is_dir() {
        return Err(ImportError {
            path: root.to_path_buf(),
            reason: "目录不存在或不是目录".to_string(),
        });
    }
    let mut plan = ImportPlan {
        kind: ImportKind::Markdown,
        ..Default::default()
    };
    let walker = walkdir::WalkDir::new(root)
        .max_depth(options.max_depth)
        .sort_by_file_name();

    for entry in walker {
        let entry = entry.map_err(|e| ImportError {
            path: root.to_path_buf(),
            reason: format!("遍历失败: {e}"),
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.into_path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !(name.ends_with(".md") || name.ends_with(".markdown")) {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let is_hidden = relative
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
        if is_hidden && !options.include_hidden {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|e| ImportError {
            path: path.clone(),
            reason: format!("读取失败: {e}"),
        })?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.clone());
        let content = String::from_utf8_lossy(&bytes);
        let (title, _, _) = markdown::md_to_note(&content, &stem);
        let hash = content_hash_hex(&bytes);
        plan.total_bytes += bytes.len() as u64;
        plan.items.push(PlanItem {
            source: relative.to_string_lossy().into_owned(),
            title,
            content_hash: hash,
            bytes: bytes.len() as u64,
            resources: 0,
        });
    }
    Ok(plan)
}

/// 预扫 ENEX 文件：每 `<note>` 一个条目（source = `<文件名>#<标题>`）。
///
/// # Errors
/// 读取失败 / XML 损坏。
pub fn plan_enex_file(enex_path: &Path) -> Result<ImportPlan, ImportError> {
    let raw = std::fs::read_to_string(enex_path).map_err(|e| ImportError {
        path: enex_path.to_path_buf(),
        reason: format!("读取失败: {e}"),
    })?;
    let notes = enex::parse_enex(&raw).map_err(|e| ImportError {
        path: enex_path.to_path_buf(),
        reason: format!("ENEX 解析失败: {e}"),
    })?;
    let file_label = enex_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export.enex".to_string());
    let mut plan = ImportPlan {
        kind: ImportKind::Enex,
        ..Default::default()
    };
    for (k, note) in notes.iter().enumerate() {
        let body_bytes = note.content.as_bytes();
        let mut res_bytes = 0u64;
        for r in &note.resources {
            res_bytes += r.data_base64.len() as u64;
        }
        let hash_src = [note.title.as_bytes(), b"\x00", body_bytes].concat();
        plan.total_bytes += body_bytes.len() as u64 + res_bytes;
        plan.items.push(PlanItem {
            source: format!("{file_label}#{k}"),
            title: note.title.clone(),
            content_hash: content_hash_hex(&hash_src),
            bytes: body_bytes.len() as u64,
            resources: note.resources.len(),
        });
    }
    Ok(plan)
}

/// 预扫 OPML 文件：每顶层 `<outline>` 子树一个条目。
///
/// # Errors
/// 读取失败 / XML 损坏。
pub fn plan_opml_file(opml_path: &Path) -> Result<ImportPlan, ImportError> {
    let raw = std::fs::read_to_string(opml_path).map_err(|e| ImportError {
        path: opml_path.to_path_buf(),
        reason: format!("读取失败: {e}"),
    })?;
    let roots = crate::opml::parse_opml(&raw).map_err(|e| ImportError {
        path: opml_path.to_path_buf(),
        reason: format!("OPML 解析失败: {e}"),
    })?;
    let file_label = opml_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "outline.opml".to_string());
    let mut plan = ImportPlan {
        kind: ImportKind::Opml,
        ..Default::default()
    };
    for (k, root) in roots.iter().enumerate() {
        let mut nodes = 0usize;
        count_outline(root, &mut nodes);
        // source 键与 import_opml_file 对齐：<文件名>#<idx>（标题可重复）
        let src = format!("{file_label}#{k}");
        plan.items.push(PlanItem {
            content_hash: content_hash_hex(format!("{}\u{1f}{}", root.text, nodes).as_bytes()),
            source: src,
            title: root.text.clone(),
            bytes: nodes as u64,
            resources: 0,
        });
        plan.total_bytes += nodes as u64;
    }
    Ok(plan)
}

fn count_outline(node: &crate::opml::OutlineNode, acc: &mut usize) {
    *acc += 1;
    for c in &node.children {
        count_outline(c, acc);
    }
}
