//! DK-3.3 拼音首字母折叠——命令面板/检索的中文标题模糊召回底座。
//!
//! `pinyin_initials`：逐字符折叠——汉字取拼音首字母（小写 ASCII），
//! 非汉字（字母/数字/符号）小写原样保留。例：「会议记录」→ "hyjl"，
//! "API 设计" → "apisj"。
//!
//! 体积注记：pinyin crate 静态字典进二进制（数百 KB 级），无运行时 IO；
//! 前端 bundle 零膨胀（对比 pinyin-pro ~90KB gz 进 bundle 的方案裁决）。

use pinyin::ToPinyin;

/// 折叠字符串为拼音首字母串（汉字→首字母小写；其余字符小写原样）。
pub fn pinyin_initials(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch.to_pinyin() {
            Some(p) => out.push_str(&p.first_letter().to_ascii_lowercase()),
            None => {
                for lc in ch.to_lowercase() {
                    out.push(lc);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::pinyin_initials;

    #[test]
    fn folds_common_titles() {
        assert_eq!(pinyin_initials("会议记录"), "hyjl");
        assert_eq!(pinyin_initials("待办清单"), "dbqd");
        // 混排：非汉字（含空格）小写原样保留，汉字折叠首字母
        assert_eq!(pinyin_initials("API 设计"), "api sj");
        assert_eq!(pinyin_initials("2026 计划"), "2026 jh");
        // 多音字取默认读音（「重」默认 zhong——crate 字典序为准，只断言折叠形态）
        let folded = pinyin_initials("重庆");
        assert_eq!(folded.chars().count(), 2);
        assert!(folded.chars().all(|c| c.is_ascii_lowercase()));
        // 纯 ASCII 恒等（小写化）
        assert_eq!(pinyin_initials("Rust Notes"), "rust notes");
        // 空串/无拼音字符
        assert_eq!(pinyin_initials(""), "");
        assert_eq!(pinyin_initials("…！"), "…！");
    }
}
