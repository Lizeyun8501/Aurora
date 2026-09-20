//! DK-09 导入结果报告：扫描/导入/跳过/失败计数 + 逐文件错误与警告。
//!
//! 报告是导入器的用户可见产物（M2 CLI / 桌面接线直接展示），
//! `Display` 输出一行摘要供日志使用。

use std::fmt;
use std::path::PathBuf;

/// 单条导入失败（文件级，不阻断其余文件）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    /// 出错的源文件路径。
    pub path: PathBuf,
    /// 失败原因（人类可读）。
    pub reason: String,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.reason)
    }
}

impl std::error::Error for ImportError {}

/// 导入结果报告。
///
/// 计数关系：`scanned` = 被视为 .md 候选的文件数；
/// `scanned = imported + failed`；`skipped` = 非 .md 或隐藏文件数（不计入 scanned）。
#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    /// 被扫描的 .md 文件数。
    pub scanned: usize,
    /// 成功导入的笔记数。
    pub imported: usize,
    /// 跳过数（非 .md 或隐藏文件/目录内文件）。
    pub skipped: usize,
    /// 导入失败的 .md 文件数。
    pub failed: usize,
    /// 成功导入的笔记 ID（与创建顺序一致）。
    pub note_ids: Vec<String>,
    /// 逐文件失败明细。
    pub errors: Vec<ImportError>,
    /// 警告（如 frontmatter 无 title、未闭合 frontmatter）。
    pub warnings: Vec<String>,
    /// 总耗时（毫秒）。
    pub duration_ms: u128,
}

impl ImportReport {
    /// 是否存在失败文件。
    pub fn has_failures(&self) -> bool {
        self.failed > 0
    }
}

impl fmt::Display for ImportReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "import: scanned={} imported={} skipped={} failed={} warnings={} {}ms",
            self.scanned,
            self.imported,
            self.skipped,
            self.failed,
            self.warnings.len(),
            self.duration_ms
        )
    }
}
