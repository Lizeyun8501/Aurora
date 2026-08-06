//! 内容编辑系统（Content Editor System）
//!
//! 实现块级文档模型、块类型注册表、Markdown 支持、版本历史、评论批注。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use uuid::Uuid;

/// 文档唯一标识
pub type DocId = String;
/// 块唯一标识
pub type BlockId = String;

/// 文档结构
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub id: DocId,
    pub title: String,
    pub blocks: Vec<Block>,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub version: u64,
}

impl Document {
    pub fn new(title: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            blocks: Vec::new(),
            properties: HashMap::new(),
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }

    pub fn with_block(mut self, block: Block) -> Self {
        self.blocks.push(block);
        self
    }

    pub fn find_block(&self, block_id: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == block_id)
    }

    pub fn find_block_mut(&mut self, block_id: &str) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| b.id == block_id)
    }

    pub fn remove_block(&mut self, block_id: &str) -> Option<Block> {
        let pos = self.blocks.iter().position(|b| b.id == block_id)?;
        Some(self.blocks.remove(pos))
    }

    pub fn insert_block_at(&mut self, index: usize, block: Block) {
        if index > self.blocks.len() {
            self.blocks.push(block);
        } else {
            self.blocks.insert(index, block);
        }
        self.updated_at = chrono::Utc::now();
        self.version += 1;
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        if !self.title.is_empty() {
            md.push_str(&format!("# {}\n\n", self.title));
        }
        for block in &self.blocks {
            md.push_str(&block.to_markdown());
            md.push('\n');
        }
        md
    }
}

/// 块结构
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Block {
    pub id: BlockId,
    pub block_type: BlockType,
    pub content: serde_json::Value,
    pub properties: HashMap<String, serde_json::Value>,
    pub children: Vec<Block>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Block {
    pub fn new(block_type: BlockType, content: impl Serialize) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            block_type,
            content: serde_json::to_value(content).unwrap_or_default(),
            properties: HashMap::new(),
            children: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn text(content: impl Into<String>) -> Self {
        Self::new(BlockType::Text, content.into())
    }

    pub fn heading(level: u8, content: impl Into<String>) -> Self {
        let mut block = Self::new(BlockType::Heading, content.into());
        block.properties.insert("level".to_string(), serde_json::json!(level.min(6).max(1)));
        block
    }

    pub fn code(language: impl Into<String>, code: impl Into<String>) -> Self {
        let mut block = Self::new(BlockType::Code, code.into());
        block.properties.insert("language".to_string(), serde_json::json!(language.into()));
        block
    }

    pub fn image(url: impl Into<String>, alt: impl Into<String>) -> Self {
        let mut block = Self::new(BlockType::Image, url.into());
        block.properties.insert("alt".to_string(), serde_json::json!(alt.into()));
        block
    }

    pub fn todo(checked: bool, content: impl Into<String>) -> Self {
        let mut block = Self::new(BlockType::TodoItem, content.into());
        block.properties.insert("checked".to_string(), serde_json::json!(checked));
        block
    }

    pub fn divider() -> Self {
        Self::new(BlockType::Divider, "")
    }

    pub fn quote(content: impl Into<String>) -> Self {
        Self::new(BlockType::Quote, content.into())
    }

    pub fn list_item(content: impl Into<String>) -> Self {
        Self::new(BlockType::ListItem, content.into())
    }

    pub fn table(rows: Vec<Vec<String>>) -> Self {
        Self::new(BlockType::Table, rows)
    }

    pub fn to_markdown(&self) -> String {
        match self.block_type {
            BlockType::Text => {
                self.content.as_str().unwrap_or("").to_string()
            }
            BlockType::Heading => {
                let level = self.properties.get("level")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as usize;
                let hashes = "#".repeat(level);
                format!("{} {}", hashes, self.content.as_str().unwrap_or(""))
            }
            BlockType::Code => {
                let lang = self.properties.get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("```{lang}\n{}\n```", self.content.as_str().unwrap_or(""))
            }
            BlockType::Image => {
                let alt = self.properties.get("alt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("![{}]({})", alt, self.content.as_str().unwrap_or(""))
            }
            BlockType::TodoItem => {
                let checked = self.properties.get("checked")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let mark = if checked { "[x]" } else { "[ ]" };
                format!("- {} {}", mark, self.content.as_str().unwrap_or(""))
            }
            BlockType::Divider => "---".to_string(),
            BlockType::Quote => {
                let text = self.content.as_str().unwrap_or("");
                text.lines().map(|l| format!("> {}", l)).collect::<Vec<_>>().join("\n")
            }
            BlockType::ListItem => {
                format!("- {}", self.content.as_str().unwrap_or(""))
            }
            BlockType::Table => {
                if let Some(rows) = self.content.as_array() {
                    let mut md = String::new();
                    for (i, row) in rows.iter().enumerate() {
                        if let Some(cells) = row.as_array() {
                            let cells_str: Vec<String> = cells.iter()
                                .map(|c| c.as_str().unwrap_or("").to_string())
                                .collect();
                            md.push_str(&format!("| {} |\n", cells_str.join(" | ")));
                            if i == 0 {
                                let sep: Vec<String> = cells.iter().map(|_| "---".to_string()).collect();
                                md.push_str(&format!("| {} |\n", sep.join(" | ")));
                            }
                        }
                    }
                    md
                } else {
                    String::new()
                }
            }
            BlockType::Custom(ref name) => {
                format!("<!-- custom:{} -->\n{}", name, self.content.as_str().unwrap_or(""))
            }
        }
    }
}

/// 块类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Text,
    Heading,
    Code,
    Image,
    Table,
    Divider,
    Quote,
    ListItem,
    TodoItem,
    #[serde(rename = "custom")]
    Custom(String),
}

impl std::fmt::Display for BlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockType::Text => write!(f, "text"),
            BlockType::Heading => write!(f, "heading"),
            BlockType::Code => write!(f, "code"),
            BlockType::Image => write!(f, "image"),
            BlockType::Table => write!(f, "table"),
            BlockType::Divider => write!(f, "divider"),
            BlockType::Quote => write!(f, "quote"),
            BlockType::ListItem => write!(f, "list_item"),
            BlockType::TodoItem => write!(f, "todo_item"),
            BlockType::Custom(name) => write!(f, "custom:{}", name),
        }
    }
}

/// 块类型注册表
#[derive(Debug, Clone)]
pub struct BlockTypeRegistry {
    types: HashMap<String, BlockTypeDef>,
}

impl Default for BlockTypeRegistry {
    fn default() -> Self {
        let mut registry = Self {
            types: HashMap::new(),
        };
        registry.register_builtin_types();
        registry
    }
}

impl BlockTypeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn register_builtin_types(&mut self) {
        self.register(BlockTypeDef {
            name: "text".to_string(),
            display_name: "文本".to_string(),
            icon: "type".to_string(),
            schema: serde_json::json!({"type": "string"}),
            default_props: HashMap::new(),
        });
        self.register(BlockTypeDef {
            name: "heading".to_string(),
            display_name: "标题".to_string(),
            icon: "heading".to_string(),
            schema: serde_json::json!({"type": "string"}),
            default_props: {
                let mut m = HashMap::new();
                m.insert("level".to_string(), serde_json::json!(1));
                m
            },
        });
        self.register(BlockTypeDef {
            name: "code".to_string(),
            display_name: "代码块".to_string(),
            icon: "code".to_string(),
            schema: serde_json::json!({"type": "string"}),
            default_props: {
                let mut m = HashMap::new();
                m.insert("language".to_string(), serde_json::json!("plaintext"));
                m
            },
        });
        self.register(BlockTypeDef {
            name: "image".to_string(),
            display_name: "图片".to_string(),
            icon: "image".to_string(),
            schema: serde_json::json!({"type": "string", "format": "uri"}),
            default_props: {
                let mut m = HashMap::new();
                m.insert("alt".to_string(), serde_json::json!(""));
                m
            },
        });
        self.register(BlockTypeDef {
            name: "table".to_string(),
            display_name: "表格".to_string(),
            icon: "table".to_string(),
            schema: serde_json::json!({"type": "array"}),
            default_props: HashMap::new(),
        });
        self.register(BlockTypeDef {
            name: "divider".to_string(),
            display_name: "分割线".to_string(),
            icon: "minus".to_string(),
            schema: serde_json::json!({"type": "string"}),
            default_props: HashMap::new(),
        });
        self.register(BlockTypeDef {
            name: "quote".to_string(),
            display_name: "引用".to_string(),
            icon: "quote".to_string(),
            schema: serde_json::json!({"type": "string"}),
            default_props: HashMap::new(),
        });
        self.register(BlockTypeDef {
            name: "list_item".to_string(),
            display_name: "列表项".to_string(),
            icon: "list".to_string(),
            schema: serde_json::json!({"type": "string"}),
            default_props: HashMap::new(),
        });
        self.register(BlockTypeDef {
            name: "todo_item".to_string(),
            display_name: "待办项".to_string(),
            icon: "check-square".to_string(),
            schema: serde_json::json!({"type": "string"}),
            default_props: {
                let mut m = HashMap::new();
                m.insert("checked".to_string(), serde_json::json!(false));
                m
            },
        });
    }

    pub fn register(&mut self, def: BlockTypeDef) {
        self.types.insert(def.name.clone(), def);
    }

    pub fn get(&self, name: &str) -> Option<&BlockTypeDef> {
        self.types.get(name)
    }

    pub fn unregister(&mut self, name: &str) {
        self.types.remove(name);
    }

    pub fn list(&self) -> Vec<&BlockTypeDef> {
        self.types.values().collect()
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }
}

/// 块类型定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTypeDef {
    pub name: String,
    pub display_name: String,
    pub icon: String,
    pub schema: serde_json::Value,
    pub default_props: HashMap<String, serde_json::Value>,
}

/// 评论/批注模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub doc_id: DocId,
    pub block_id: Option<BlockId>,
    pub anchor: CommentAnchor,
    pub author_id: String,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolved: bool,
    pub replies: Vec<CommentReply>,
}

impl Comment {
    pub fn new(doc_id: DocId, author_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            doc_id,
            block_id: None,
            anchor: CommentAnchor::Document,
            author_id: author_id.into(),
            content: content.into(),
            created_at: chrono::Utc::now(),
            resolved: false,
            replies: Vec::new(),
        }
    }

    pub fn on_block(mut self, block_id: BlockId) -> Self {
        self.block_id = Some(block_id);
        self.anchor = CommentAnchor::Block;
        self
    }

    pub fn with_range(mut self, start: usize, end: usize) -> Self {
        self.anchor = CommentAnchor::TextRange { start, end };
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommentAnchor {
    Document,
    Block,
    TextRange { start: usize, end: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentReply {
    pub id: String,
    pub author_id: String,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 文档版本历史快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSnapshot {
    pub id: String,
    pub doc_id: DocId,
    pub version: u64,
    pub document: Document,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
    pub comment: Option<String>,
}

/// 内容编辑引擎
#[derive(Debug, Clone)]
pub struct ContentEditorEngine {
    registry: Arc<RwLock<BlockTypeRegistry>>,
    documents: Arc<RwLock<HashMap<DocId, Document>>>,
    snapshots: Arc<RwLock<HashMap<DocId, Vec<DocumentSnapshot>>>>,
    comments: Arc<RwLock<HashMap<String, Comment>>>,
}

impl Default for ContentEditorEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentEditorEngine {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(BlockTypeRegistry::new())),
            documents: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            comments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn create_document(&self, title: impl Into<String>) -> Document {
        let doc = Document::new(title);
        self.documents.write().insert(doc.id.clone(), doc.clone());
        doc
    }

    pub fn get_document(&self, doc_id: &str) -> Option<Document> {
        self.documents.read().get(doc_id).cloned()
    }

    pub fn update_document(&self, doc: Document) {
        self.documents.write().insert(doc.id.clone(), doc);
    }

    pub fn delete_document(&self, doc_id: &str) -> Option<Document> {
        self.documents.write().remove(doc_id)
    }

    pub fn list_documents(&self) -> Vec<Document> {
        self.documents.read().values().cloned().collect()
    }

    pub fn add_block(&self, doc_id: &str, block: Block) -> Option<Block> {
        let mut docs = self.documents.write();
        let doc = docs.get_mut(doc_id)?;
        doc.blocks.push(block.clone());
        doc.updated_at = chrono::Utc::now();
        doc.version += 1;
        Some(block)
    }

    pub fn update_block(&self, doc_id: &str, block: Block) -> Option<Block> {
        let mut docs = self.documents.write();
        let doc = docs.get_mut(doc_id)?;
        if let Some(existing) = doc.find_block_mut(&block.id) {
            *existing = block.clone();
            doc.updated_at = chrono::Utc::now();
            doc.version += 1;
            Some(block)
        } else {
            None
        }
    }

    pub fn remove_block(&self, doc_id: &str, block_id: &str) -> Option<Block> {
        let mut docs = self.documents.write();
        let doc = docs.get_mut(doc_id)?;
        let block = doc.remove_block(block_id)?;
        doc.updated_at = chrono::Utc::now();
        doc.version += 1;
        Some(block)
    }

    pub fn move_block(&self, doc_id: &str, block_id: &str, new_index: usize) -> Option<()> {
        let mut docs = self.documents.write();
        let doc = docs.get_mut(doc_id)?;
        let old_index = doc.blocks.iter().position(|b| b.id == block_id)?;
        if old_index == new_index || new_index >= doc.blocks.len() {
            return Some(());
        }
        let block = doc.blocks.remove(old_index);
        let insert_idx = new_index.min(doc.blocks.len());
        doc.blocks.insert(insert_idx, block);
        doc.updated_at = chrono::Utc::now();
        doc.version += 1;
        Some(())
    }

    /// 创建文档快照
    pub fn create_snapshot(&self, doc_id: &str, created_by: impl Into<String>, comment: Option<String>) -> Option<DocumentSnapshot> {
        let doc = self.documents.read().get(doc_id)?.clone();
        let snapshot = DocumentSnapshot {
            id: Uuid::new_v4().to_string(),
            doc_id: doc_id.to_string(),
            version: doc.version,
            document: doc,
            created_at: chrono::Utc::now(),
            created_by: created_by.into(),
            comment,
        };
        self.snapshots.write()
            .entry(doc_id.to_string())
            .or_default()
            .push(snapshot.clone());
        Some(snapshot)
    }

    pub fn get_snapshots(&self, doc_id: &str) -> Vec<DocumentSnapshot> {
        self.snapshots.read()
            .get(doc_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn restore_snapshot(&self, snapshot_id: &str) -> Option<Document> {
        let snapshots = self.snapshots.read();
        let found = snapshots.iter().find_map(|(doc_id, list)| {
            list.iter().find(|s| s.id == snapshot_id).map(|snap| (doc_id.clone(), snap.document.clone()))
        });
        drop(snapshots);
        found.map(|(doc_id, doc)| {
            self.documents.write().insert(doc_id, doc.clone());
            doc
        })
    }

    /// 添加评论
    pub fn add_comment(&self, comment: Comment) -> Comment {
        let c = comment.clone();
        self.comments.write().insert(comment.id.clone(), comment);
        c
    }

    pub fn get_comment(&self, comment_id: &str) -> Option<Comment> {
        self.comments.read().get(comment_id).cloned()
    }

    pub fn list_comments(&self, doc_id: &str) -> Vec<Comment> {
        self.comments.read()
            .values()
            .filter(|c| c.doc_id == doc_id)
            .cloned()
            .collect()
    }

    pub fn resolve_comment(&self, comment_id: &str) -> Option<Comment> {
        let mut comments = self.comments.write();
        let comment = comments.get_mut(comment_id)?;
        comment.resolved = true;
        Some(comment.clone())
    }

    pub fn registry(&self) -> Arc<RwLock<BlockTypeRegistry>> {
        self.registry.clone()
    }

    /// Markdown 导入：简单解析
    pub fn import_markdown(&self, title: impl Into<String>, md: &str) -> Document {
        let mut doc = Document::new(title);
        let mut current_code: Option<(String, String)> = None;

        for line in md.lines() {
            if let Some((lang, code)) = current_code.as_mut() {
                if line.starts_with("```") {
                    doc.blocks.push(Block::code(lang.clone(), code.clone()));
                    current_code = None;
                } else {
                    code.push_str(line);
                    code.push('\n');
                }
                continue;
            }

            if line.starts_with("```") {
                let lang = line.trim_start_matches("`").trim().to_string();
                current_code = Some((lang, String::new()));
                continue;
            }

            if line.starts_with("---") || line.starts_with("***") {
                doc.blocks.push(Block::divider());
            } else if line.starts_with("# ") {
                doc.blocks.push(Block::heading(1, &line[2..]));
            } else if line.starts_with("## ") {
                doc.blocks.push(Block::heading(2, &line[3..]));
            } else if line.starts_with("### ") {
                doc.blocks.push(Block::heading(3, &line[4..]));
            } else if line.starts_with("> ") {
                doc.blocks.push(Block::quote(&line[2..]));
            } else if line.starts_with("- [ ] ") {
                doc.blocks.push(Block::todo(false, &line[6..]));
            } else if line.starts_with("- [x] ") || line.starts_with("- [X] ") {
                doc.blocks.push(Block::todo(true, &line[6..]));
            } else if line.starts_with("- ") {
                doc.blocks.push(Block::list_item(&line[2..]));
            } else if line.starts_with("! [") {
                if let Some((alt, url)) = Self::parse_markdown_image(line) {
                    doc.blocks.push(Block::image(url, alt));
                }
            } else if !line.trim().is_empty() {
                doc.blocks.push(Block::text(line));
            }
        }

        if let Some((lang, code)) = current_code {
            doc.blocks.push(Block::code(lang, code));
        }

        self.documents.write().insert(doc.id.clone(), doc.clone());
        doc
    }

    fn parse_markdown_image(line: &str) -> Option<(String, String)> {
        let start = line.find("![")?;
        let mid = line.find("](")?;
        let end = line.find(")")?;
        if start < mid && mid < end {
            let alt = line[start + 2..mid].to_string();
            let url = line[mid + 2..end].to_string();
            Some((alt, url))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_crud() {
        let engine = ContentEditorEngine::new();
        let doc = engine.create_document("Test Doc");
        assert_eq!(doc.title, "Test Doc");

        let block = Block::text("Hello world");
        engine.add_block(&doc.id, block.clone());

        let retrieved = engine.get_document(&doc.id).unwrap();
        assert_eq!(retrieved.blocks.len(), 1);
        assert_eq!(retrieved.blocks[0].content, serde_json::json!("Hello world"));
    }

    #[test]
    fn test_block_to_markdown() {
        let h1 = Block::heading(1, "Title");
        assert_eq!(h1.to_markdown(), "# Title");

        let code = Block::code("rust", "let x = 1;");
        assert_eq!(code.to_markdown(), "```rust\nlet x = 1;\n```");

        let todo = Block::todo(false, "Buy milk");
        assert_eq!(todo.to_markdown(), "- [ ] Buy milk");
    }

    #[test]
    fn test_markdown_import() {
        let engine = ContentEditorEngine::new();
        let md = "# Hello\n\nThis is a paragraph.\n\n- [ ] Task 1\n- [x] Task 2\n\n```rust\nfn main() {}\n```\n\n> Quote\n";
        let doc = engine.import_markdown("Test", md);
        assert_eq!(doc.blocks.len(), 6);
        assert_eq!(doc.blocks[0].block_type, BlockType::Heading);
        assert_eq!(doc.blocks[2].block_type, BlockType::TodoItem);
        assert_eq!(doc.blocks[4].block_type, BlockType::Code);
        assert_eq!(doc.blocks[5].block_type, BlockType::Quote);
    }

    #[test]
    fn test_snapshot_restore() {
        let engine = ContentEditorEngine::new();
        let doc = engine.create_document("Original");
        engine.add_block(&doc.id, Block::text("v1"));

        let snap = engine.create_snapshot(&doc.id, "user1", Some("first draft".to_string())).unwrap();

        engine.add_block(&doc.id, Block::text("v2"));
        let current = engine.get_document(&doc.id).unwrap();
        assert_eq!(current.blocks.len(), 2);

        let restored = engine.restore_snapshot(&snap.id).unwrap();
        assert_eq!(restored.blocks.len(), 1);
    }

    #[test]
    fn test_comments() {
        let engine = ContentEditorEngine::new();
        let doc = engine.create_document("Doc");
        let comment = Comment::new(doc.id.clone(), "user1", "Looks good");
        let comment = engine.add_comment(comment);
        assert!(!comment.resolved);

        let resolved = engine.resolve_comment(&comment.id).unwrap();
        assert!(resolved.resolved);

        let comments = engine.list_comments(&doc.id);
        assert_eq!(comments.len(), 1);
    }

    #[test]
    fn test_block_registry() {
        let registry = BlockTypeRegistry::new();
        assert!(registry.is_registered("text"));
        assert!(registry.is_registered("todo_item"));
        assert!(!registry.is_registered("unknown"));

        let types = registry.list();
        assert_eq!(types.len(), 9);
    }
}
