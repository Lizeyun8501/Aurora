//! Markdown → 笔记 内容变换：YAML frontmatter 剥离 + 标题提取。
//!
//! 设计约束（任务书 §1 道德底线）：**导入不得有格式损失** — 代码块、
//! 表格、任务列表等 body 内容逐字符原样保留（我们存的就是 Markdown 体）。
//! 变换仅限两处：frontmatter 块剥离（避免渲染出 `---` 横线）、标题确定
//! （frontmatter `title:` 优先，否则文件名 stem）。
//!
//! YAML 解析刻意自写最小子集（仅顶层 `key: value` 标量），不引入
//! serde_yaml 依赖 — frontmatter 解析失败一律**原样导入 + 警告**，
//! 绝不丢内容（fail-open 到无损方向）。

/// frontmatter 剥离结果：`(Some(frontmatter 文本), 剩余正文)` 或 `(None, 原文)`。
///
/// 仅当文件第一行恰为 `---`（容忍 `\r\n`）时视为 frontmatter 开头；
/// 结束行为下一个独立 `---` 行。未闭合不算 frontmatter（防误剥正文）。
/// body 通过 `split_inclusive` 切片，**逐字节保留**原行尾。
pub fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let Some(first_eol) = content.find('\n') else {
        return (None, content);
    };
    if content[..first_eol].trim_end_matches('\r') != "---" {
        return (None, content);
    }
    let rest = &content[first_eol + 1..];
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == "---" {
            let fm = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (Some(fm), body);
        }
        offset += line.len();
    }
    (None, content)
}

/// 从 frontmatter 文本提取顶层 `title:` 标量值（去引号、trim）。
///
/// 仅接受无缩进的 `title:` 行（顶层键）；空值/纯引号视为缺失。
pub fn extract_title(frontmatter: &str) -> Option<String> {
    for line in frontmatter.lines() {
        if !line.starts_with("title:") {
            continue;
        }
        let value = line["title:".len()..].trim();
        let unquoted = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        let title = unquoted.trim();
        if !title.is_empty() {
            return Some(title.to_string());
        }
        return None;
    }
    None
}

/// Markdown 文件内容 + 文件名 stem → `(标题, 导入正文, 警告列表)`。
///
/// - frontmatter 有合法 `title:` → 用它；
/// - frontmatter 存在但无 title → 文件名 stem + 警告；
/// - frontmatter 未闭合 → 原样导入（正文无损）+ 警告；
/// - 无 frontmatter → 文件名 stem。
pub fn md_to_note(content: &str, stem: &str) -> (String, String, Vec<String>) {
    let mut warnings = Vec::new();
    let (frontmatter, body) = split_frontmatter(content);
    let title = match frontmatter {
        Some(fm) => match extract_title(fm) {
            Some(t) => t,
            None => {
                warnings.push("frontmatter 存在但无 title 字段，使用文件名作为标题".to_string());
                stem.to_string()
            }
        },
        None => {
            if content.starts_with("---") {
                warnings.push("frontmatter 未闭合，按正文原样导入".to_string());
            }
            stem.to_string()
        }
    };
    (title, body.to_string(), warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_basic_split() {
        let src = "---\ntitle: A\n---\nbody\n";
        let (fm, body) = split_frontmatter(src);
        assert_eq!(fm, Some("title: A\n"));
        assert_eq!(body, "body\n");
    }

    #[test]
    fn frontmatter_unclosed_is_body() {
        let src = "---\ntitle: A\nno closing\n";
        let (fm, body) = split_frontmatter(src);
        assert!(fm.is_none());
        assert_eq!(body, src);
    }

    #[test]
    fn title_extraction_quotes_and_indent() {
        let fm = "tags: [x]\ntitle: \"My Note\"\nother: 1\n";
        assert_eq!(extract_title(fm), Some("My Note".to_string()));
        // 缩进的 title: 不是顶层键
        assert_eq!(extract_title("  title: deep\n"), None);
    }

    #[test]
    fn md_to_note_no_loss_without_frontmatter() {
        let body = "# H\n\n```rust\ncode---\n```\n";
        let (title, out, warnings) = md_to_note(body, "f");
        assert_eq!(title, "f");
        assert_eq!(out, body);
        assert!(warnings.is_empty());
    }
}
