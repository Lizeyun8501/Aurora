//! DK-14e.1 示范插件 · 计算型模板：plugin-wordcount-guest。
//!
//! 绑定自 `../aurora-plugin/wit/aurora-plugin.wit`（单一事实源）生成。
//!
//! 行为约定（docs/plugin-log-conventions.md）：handle-event 解析 payload
//! JSON（取 `text` 字段，缺省回退整段 payload），统计字数/字符数/行数，
//! 输出**单行 JSON**（测试可断言第一判据）：
//! `[wordcount] {"chars":N,"words":N,"lines":N}`
//!
//! 工程结构同 hello 模板：逻辑在 [`logic`]（host 可测），绑定 cfg 隔离。

pub mod logic {
    /// 计数核心：(chars, words, lines)。
    /// words=空白分隔词数；lines=非空文本按 \n 分行数（空文本为 0）。
    pub fn count(text: &str) -> (usize, usize, usize) {
        let chars = text.chars().count();
        let words = text.split_whitespace().count();
        let lines = if text.is_empty() {
            0
        } else {
            text.lines().count()
        };
        (chars, words, lines)
    }

    /// payload JSON 提取：对象含 "text" 字符串字段则取之，否则整段回退。
    pub fn extract_text(payload: &str) -> String {
        serde_json::from_str::<serde_json::Value>(payload)
            .ok()
            .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_else(|| payload.to_string())
    }

    /// 单行 JSON 输出行（测试可断言第一判据）。
    pub fn output_line(text: &str) -> String {
        let (c, w, l) = count(text);
        format!("[wordcount] {{\"chars\":{c},\"words\":{w},\"lines\":{l}}}")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn count_english_basic() {
            // "hello world"(11) + "\n"(1) + "second line"(11) = 23 chars
            assert_eq!(count("hello world\nsecond line"), (23, 4, 2));
        }

        #[test]
        fn count_empty_is_zero() {
            assert_eq!(count(""), (0, 0, 0));
        }

        #[test]
        fn count_unicode() {
            assert_eq!(count("你好世界"), (4, 1, 1));
        }

        #[test]
        fn count_trailing_newline() {
            assert_eq!(count("a\n"), (2, 1, 1));
        }

        #[test]
        fn count_multiline_words() {
            assert_eq!(count("a b\nc  d\n"), (9, 4, 2));
        }

        #[test]
        fn extract_text_takes_field() {
            assert_eq!(extract_text(r#"{"text":"a b","id":7}"#), "a b");
        }

        #[test]
        fn extract_text_fallback_raw() {
            assert_eq!(extract_text(r#"{"other":1}"#), r#"{"other":1}"#);
        }

        #[test]
        fn output_line_is_single_line_json() {
            let line = output_line("one two\nthree");
            assert_eq!(line, "[wordcount] {\"chars\":13,\"words\":3,\"lines\":2}");
            assert!(!line.contains('\n'));
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_exports {
    wit_bindgen::generate!({
        path: "../aurora-plugin/wit",
        world: "plugin",
    });

    struct WordcountGuest;

    impl exports::aurora::plugin::host::Guest for WordcountGuest {
        fn init(
            _capability: &exports::aurora::plugin::host::CapabilityToken,
            _plugin_id: String,
        ) -> Result<(), ()> {
            eprintln!("[wordcount] init ok");
            Ok(())
        }

        fn handle_event(
            _capability: &exports::aurora::plugin::host::CapabilityToken,
            _event: String,
            payload: String,
        ) -> Result<(), ()> {
            eprintln!(
                "{}",
                super::logic::output_line(&super::logic::extract_text(&payload))
            );
            Ok(())
        }
    }

    export!(WordcountGuest);
}
