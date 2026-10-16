//! Markdown 导出原语（DK-审阅 P0-3：导出面=用户信任承诺缺口首切片）。
//!
//! 职责：文件名安全化 + 重名去重（纯函数，desktop 命令层薄委托）。
//! 语义边界见 issues/wf/alpha-导出面注记.md（v1：明文 md 出库为用户
//! 显式行为，确认门归 UI 层；附件不导出）。

use std::collections::HashSet;

/// 非法文件名字符集（Windows/Linux/macOS 交集保守面）+ 控制字符。
fn is_unsafe_filename_byte(b: u8) -> bool {
    matches!(
        b,
        b'/' | b'\\' | b':' | b'*' | b'?' | b'"' | b'<' | b'>' | b'|'
    ) || b < 0x20
}

/// 标题 → 安全文件名基名（不含扩展名）。
///
/// 非法字符→`_`；首尾空白/点去除（Windows 尾点陷阱）；空/全非法→`untitled`；
/// 长度 cap 100 字符（字符边界截断，防多字节劈半）。
pub fn sanitize_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            let mut buf = [0u8; 4];
            let bytes = c.encode_utf8(&mut buf).as_bytes();
            if bytes.iter().any(|&b| is_unsafe_filename_byte(b)) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        return "untitled".to_string();
    }
    if trimmed.chars().count() <= 100 {
        return trimmed.to_string();
    }
    trimmed.chars().take(100).collect()
}

/// 重名去重：`base.md` 冲突则 `base-2.md`、`base-3.md` …（existing 为已占用
/// 基名集合——调用方先 scan 目标目录注入）。
pub fn dedupe_filename(existing: &HashSet<String>, base: &str) -> String {
    let first = format!("{base}.md");
    if !existing.contains(&first) {
        return first;
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}.md");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("u32 重名序列不可能耗尽")
}

/// 从目标目录扫描既有 `.md` 基名集合（重名去重注入源）。
pub fn existing_md_names(entries: &[String]) -> HashSet<String> {
    entries
        .iter()
        .filter(|e| e.ends_with(".md"))
        .map(|e| e.trim_end_matches(".md").to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_unsafe_chars() {
        assert_eq!(
            sanitize_filename("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
    }

    #[test]
    fn test_sanitize_trims_and_dots() {
        assert_eq!(sanitize_filename("  hello  "), "hello");
        assert_eq!(sanitize_filename("note..."), "note"); // Windows 尾点陷阱
        assert_eq!(sanitize_filename(""), "untitled");
        assert_eq!(sanitize_filename("///"), "___"); // 全非法→忠实替换（trim 不去 _——保守语义）
    }

    #[test]
    fn test_sanitize_chinese_and_control() {
        assert_eq!(sanitize_filename("会议纪要\nv2"), "会议纪要_v2");
        assert_eq!(sanitize_filename("设计\u{0}稿"), "设计_稿");
    }

    #[test]
    fn test_sanitize_length_cap_utf8_boundary() {
        let long = "汉".repeat(120); // 120 字符（240 字节）——截断 100 字符不劈半
        let out = sanitize_filename(&long);
        assert_eq!(out.chars().count(), 100);
    }

    #[test]
    fn test_dedupe_sequence() {
        let mut existing = HashSet::new();
        existing.insert("note.md".to_string());
        assert_eq!(dedupe_filename(&existing, "note"), "note-2.md");
        existing.insert("note-2.md".to_string());
        assert_eq!(dedupe_filename(&existing, "note"), "note-3.md");
        assert_eq!(dedupe_filename(&existing, "other"), "other.md");
    }

    #[test]
    fn test_existing_md_names_filter() {
        let names = vec!["a.md".to_string(), "b.txt".to_string(), "c.md".to_string()];
        let set = existing_md_names(&names);
        assert!(set.contains("a") && set.contains("c") && !set.contains("b"));
    }
}
