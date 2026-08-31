//! jieba 中文分词 tokenizer — Tantivy 自定义分词器（V20 Phase 1）
//!
//! 背景：Tantivy `SimpleTokenizer` 按空白切词，中文整句成单 token，
//! 子串查询无法命中（「架构投影验证」搜「架构」为空）。
//!
//! 方案：
//! - `jieba-rs` `TokenizeMode::Search`（搜索引擎模式：长词再切分，
//!   召回优先，适合笔记检索的索引侧）
//! - 注册为 `"jieba"` analyzer（`JiebaTokenizer + LowerCaser`），
//!   `title`/`content`/`tags` 字段显式指定该 tokenizer；
//!   `default`（SimpleTokenizer+LowerCaser）保留给旧索引/其他字段，
//!   英文场景行为不变（存量测试零回归）
//! - 词典（内置 dict.txt ~1.5MB）经 `Arc<Jieba>` 全局单例共享，
//!   Tokenizer Clone 仅复制 Arc
//!
//! 查询侧：`QueryParser` 按字段绑定的 tokenizer 分析查询词，
//! 索引/查询天然一致（「架构投影」→ ["架构", "投影"] AND 查询）。

use std::sync::Arc;

use jieba_rs::Jieba;
use tantivy::tokenizer::{LowerCaser, TextAnalyzer, Token, TokenStream, Tokenizer};

/// jieba 分词 tokenizer（Search 模式 + Arc 共享词典）。
#[derive(Clone)]
pub struct JiebaTokenizer {
    jieba: Arc<Jieba>,
}

impl Default for JiebaTokenizer {
    fn default() -> Self {
        // 内置词典加载 ~几十 ms（进程首次）；后续 Clone 走 Arc
        Self { jieba: Arc::new(Jieba::new()) }
    }
}

impl Tokenizer for JiebaTokenizer {
    type TokenStream<'a> = JiebaTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        // Search 模式: 精确模式结果 + 长词再切分（如「中华人民共和国」
        // 额外出「中华」「人民」），保证子串召回
        let tokens = self
            .jieba
            .tokenize(text, jieba_rs::TokenizeMode::Search, true)
            .into_iter()
            .map(|t| (t.word.to_string(), t.start, t.end))
            .collect::<Vec<_>>();
        JiebaTokenStream {
            tokens,
            token: Token::default(),
            index: 0,
        }
    }
}

/// jieba token 流。
pub struct JiebaTokenStream {
    tokens: Vec<(String, usize, usize)>,
    token: Token,
    index: usize,
}

impl TokenStream for JiebaTokenStream {
    fn advance(&mut self) -> bool {
        if self.index >= self.tokens.len() {
            return false;
        }
        let (word, start, end) = &self.tokens[self.index];
        self.token = Token {
            offset_from: *start,
            offset_to: *end,
            position: self.index,
            text: word.to_lowercase(), // 与 LowerCaser filter 双保险（纯 CJK 无影响）
            position_length: 1,
        };
        self.index += 1;
        true
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}

/// 注册 `"jieba"` analyzer 到索引（Jieba + 小写归一）。
///
/// 配合 schema 字段级 `set_tokenizer("jieba")` 使用；
/// 查询侧由 QueryParser 按字段 tokenizer 自动走同一 analyzer。
pub fn register_jieba(index: &tantivy::Index) {
    let analyzer = TextAnalyzer::builder(JiebaTokenizer::default())
        .filter(LowerCaser)
        .build();
    index.tokenizers().register("jieba", analyzer);
}

#[cfg(test)]
mod tests {
    use tantivy::tokenizer::Tokenizer;

    use super::*;

    #[test]
    fn jieba_splits_chinese_into_words() {
        let mut tk = JiebaTokenizer::default();
        let mut stream = tk.token_stream("架构投影验证");
        let mut words = Vec::new();
        while stream.advance() {
            words.push(stream.token().text.clone());
        }
        assert!(words.contains(&"架构".to_string()), "「架构」应被切出: {words:?}");
        assert!(words.contains(&"投影".to_string()), "「投影」应被切出: {words:?}");
    }

    #[test]
    fn jieba_handles_english_and_mixed() {
        let mut tk = JiebaTokenizer::default();
        let mut stream = tk.token_stream("Aurora 笔记 Note");
        let mut words = Vec::new();
        while stream.advance() {
            words.push(stream.token().text.clone());
        }
        // 英文单词独立成词（大小写归一）
        assert!(words.iter().any(|w| w == "aurora"), "{words:?}");
        assert!(words.iter().any(|w| w == "note"), "{words:?}");
        assert!(words.iter().any(|w| w == "笔记"), "{words:?}");
    }
}
