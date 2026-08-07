//! 知识网络系统（Knowledge Network System）
//!
//! 实现双链引用、反向链接面板、知识图谱可视化、关系属性、图谱探索。

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use uuid::Uuid;

use super::content_editor::{DocId, Document};

/// 链接唯一标识
pub type LinkId = String;
/// 节点唯一标识（对应文档ID）
pub type NodeId = String;

/// 链接类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    /// WikiLink: [[标题]]
    WikiLink,
    /// MarkdownLink: [文本](URL)
    MarkdownLink,
    /// 关系属性链接（语义标签）
    Relation,
}

impl std::fmt::Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkType::WikiLink => write!(f, "wiki_link"),
            LinkType::MarkdownLink => write!(f, "markdown_link"),
            LinkType::Relation => write!(f, "relation"),
        }
    }
}

/// 语义关系标签
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRelation {
    /// 支持
    Supports,
    /// 反驳
    Refutes,
    /// 引用
    References,
    /// 扩展
    Extends,
    /// 相关
    Related,
    /// 自定义关系
    Custom(String),
}

impl std::fmt::Display for SemanticRelation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SemanticRelation::Supports => write!(f, "supports"),
            SemanticRelation::Refutes => write!(f, "refutes"),
            SemanticRelation::References => write!(f, "references"),
            SemanticRelation::Extends => write!(f, "extends"),
            SemanticRelation::Related => write!(f, "related"),
            SemanticRelation::Custom(s) => write!(f, "custom:{}", s),
        }
    }
}

/// 链接结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub id: LinkId,
    pub source_doc_id: DocId,
    pub target_doc_id: DocId,
    pub link_type: LinkType,
    /// 语义关系标签（可选）
    pub semantic_relation: Option<SemanticRelation>,
    /// 链接在源文档中的锚文本
    pub anchor_text: Option<String>,
    /// 链接出现的块ID
    pub block_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Link {
    pub fn new(source_doc_id: DocId, target_doc_id: DocId, link_type: LinkType) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source_doc_id,
            target_doc_id,
            link_type,
            semantic_relation: None,
            anchor_text: None,
            block_id: None,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn with_semantic_relation(mut self, relation: SemanticRelation) -> Self {
        self.semantic_relation = Some(relation);
        self
    }

    pub fn with_anchor_text(mut self, text: impl Into<String>) -> Self {
        self.anchor_text = Some(text.into());
        self
    }

    pub fn on_block(mut self, block_id: impl Into<String>) -> Self {
        self.block_id = Some(block_id.into());
        self
    }
}

/// 反向链接条目（包含上下文预览）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacklinkEntry {
    pub source_doc_id: DocId,
    pub source_doc_title: String,
    pub link_id: LinkId,
    pub anchor_text: Option<String>,
    pub block_id: Option<String>,
    pub semantic_relation: Option<SemanticRelation>,
}

/// 图谱节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: NodeId,
    pub title: String,
    /// 节点度数（连接数）
    pub degree: usize,
    /// 聚类标签
    pub cluster: Option<String>,
    /// 自定义属性
    pub properties: HashMap<String, serde_json::Value>,
}

/// 图谱边
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: NodeId,
    pub target: NodeId,
    pub link_type: LinkType,
    pub semantic_relation: Option<SemanticRelation>,
    pub weight: f64,
}

/// 知识图谱
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: GraphNode) {
        if !self.nodes.iter().any(|n| n.id == node.id) {
            self.nodes.push(node);
        }
    }

    pub fn add_edge(&mut self, edge: GraphEdge) {
        if !self.edges.iter().any(|e| e.id == edge.id) {
            self.edges.push(edge);
        }
    }

    /// 获取节点的邻居节点ID
    pub fn neighbors(&self, node_id: &str) -> Vec<NodeId> {
        let mut neighbors = HashSet::new();
        for edge in &self.edges {
            if edge.source == node_id {
                neighbors.insert(edge.target.clone());
            }
            if edge.target == node_id {
                neighbors.insert(edge.source.clone());
            }
        }
        neighbors.into_iter().collect()
    }

    /// 获取节点的出边
    pub fn outgoing_edges(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.edges.iter().filter(|e| e.source == node_id).collect()
    }

    /// 获取节点的入边
    pub fn incoming_edges(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.edges.iter().filter(|e| e.target == node_id).collect()
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// 链接索引
#[derive(Debug, Clone)]
pub struct LinkIndex {
    /// 源文档 -> 链接列表
    forward: HashMap<DocId, Vec<Link>>,
    /// 目标文档 -> 反向链接列表
    backward: HashMap<DocId, Vec<BacklinkEntry>>,
    /// 文档标题 -> 文档ID 映射（用于WikiLink解析）
    title_index: HashMap<String, DocId>,
}

impl Default for LinkIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkIndex {
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
            backward: HashMap::new(),
            title_index: HashMap::new(),
        }
    }

    /// 注册文档标题
    pub fn register_document(&mut self, doc_id: DocId, title: String) {
        self.title_index.insert(title, doc_id);
    }

    /// 注销文档
    pub fn unregister_document(&mut self, doc_id: &str) {
        self.title_index.retain(|_, id| id != doc_id);
        // 清除该文档相关的所有链接
        if let Some(links) = self.forward.remove(doc_id) {
            for link in links {
                if let Some(backlinks) = self.backward.get_mut(&link.target_doc_id) {
                    backlinks.retain(|b| b.link_id != link.id);
                }
            }
        }
        // 清除指向该文档的反向链接
        self.backward.remove(doc_id);
        // 清除其他文档指向该文档的链接
        for (_, links) in self.forward.iter_mut() {
            links.retain(|l| l.target_doc_id != doc_id);
        }
    }

    /// 添加链接
    pub fn add_link(&mut self, link: Link, target_title: String) {
        let backlink = BacklinkEntry {
            source_doc_id: link.source_doc_id.clone(),
            source_doc_title: target_title,
            link_id: link.id.clone(),
            anchor_text: link.anchor_text.clone(),
            block_id: link.block_id.clone(),
            semantic_relation: link.semantic_relation.clone(),
        };

        let target_doc_id = link.target_doc_id.clone();
        self.forward
            .entry(link.source_doc_id.clone())
            .or_default()
            .push(link);
        self.backward
            .entry(target_doc_id)
            .or_default()
            .push(backlink);
    }

    /// 获取文档的出链
    pub fn get_outgoing_links(&self, doc_id: &str) -> Vec<&Link> {
        self.forward
            .get(doc_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// 获取文档的反向链接
    pub fn get_backlinks(&self, doc_id: &str) -> Vec<&BacklinkEntry> {
        self.backward
            .get(doc_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// 通过标题查找文档ID
    pub fn resolve_wiki_link(&self, title: &str) -> Option<&DocId> {
        self.title_index.get(title)
    }

    /// 获取所有链接
    pub fn all_links(&self) -> Vec<&Link> {
        self.forward.values().flatten().collect()
    }

    /// 更新文档标题
    pub fn update_title(&mut self, doc_id: &str, old_title: &str, new_title: String) {
        self.title_index.remove(old_title);
        self.title_index.insert(new_title, doc_id.to_string());
    }
}

/// 链接解析器
pub struct LinkParser;

impl LinkParser {
    /// 从文本中解析 WikiLink [[标题]]
    pub fn parse_wiki_links(text: &str) -> Vec<(String, Option<String>)> {
        let mut links = Vec::new();
        let mut chars = text.char_indices().peekable();

        while let Some((start, c)) = chars.next() {
            if c == '[' && chars.peek().map(|(_, c)| *c) == Some('[') {
                chars.next(); // consume second '['
                let link_start = start + 2;
                let mut link_end = link_start;
                let mut found = false;

                while let Some((end, c)) = chars.next() {
                    if c == ']' && chars.peek().map(|(_, c)| *c) == Some(']') {
                        chars.next(); // consume second ']'
                        link_end = end;
                        found = true;
                        break;
                    }
                }

                if found {
                    let content = &text[link_start..link_end];
                    // 支持 [[标题|显示文本]] 格式
                    if let Some(pipe_pos) = content.find('|') {
                        let title = content[..pipe_pos].trim().to_string();
                        let display = content[pipe_pos + 1..].trim().to_string();
                        links.push((title, Some(display)));
                    } else {
                        links.push((content.trim().to_string(), None));
                    }
                }
            }
        }

        links
    }

    /// 从文本中解析 MarkdownLink [文本](URL)
    pub fn parse_markdown_links(text: &str) -> Vec<(String, String)> {
        let mut links = Vec::new();
        let mut chars = text.char_indices().peekable();

        while let Some((start, c)) = chars.next() {
            if c == '[' {
                let text_start = start + 1;
                let mut text_end = text_start;
                let mut bracket_depth = 1;

                for (end, c) in chars.by_ref() {
                    if c == '[' {
                        bracket_depth += 1;
                    } else if c == ']' {
                        bracket_depth -= 1;
                        if bracket_depth == 0 {
                            text_end = end;
                            break;
                        }
                    }
                }

                if bracket_depth == 0 && chars.peek().map(|(_, c)| *c) == Some('(') {
                    chars.next(); // consume '('
                    let url_start = text_end + 2;
                    let mut url_end = url_start;

                    for (end, c) in chars.by_ref() {
                        if c == ')' {
                            url_end = end;
                            break;
                        }
                    }

                    if url_end > url_start {
                        let link_text = text[text_start..text_end].to_string();
                        let url = text[url_start..url_end].to_string();
                        links.push((link_text, url));
                    }
                }
            }
        }

        links
    }

    /// 从文档内容中提取所有链接
    pub fn extract_links_from_doc(doc: &Document) -> Vec<Link> {
        let mut links = Vec::new();

        for block in &doc.blocks {
            Self::extract_links_from_block(&doc.id, block, &mut links);
        }

        links
    }

    fn extract_links_from_block(
        doc_id: &str,
        block: &super::content_editor::Block,
        links: &mut Vec<Link>,
    ) {
        if let Some(text) = block.content.as_str() {
            // 解析 WikiLink
            for (title, _) in Self::parse_wiki_links(text) {
                let mut link = Link::new(
                    doc_id.to_string(),
                    title.clone(), // 临时存储标题，后续通过 title_index 解析
                    LinkType::WikiLink,
                );
                link.anchor_text = Some(title);
                link.block_id = Some(block.id.clone());
                links.push(link);
            }

            // 解析 MarkdownLink
            for (link_text, url) in Self::parse_markdown_links(text) {
                let mut link = Link::new(doc_id.to_string(), url.clone(), LinkType::MarkdownLink);
                link.anchor_text = Some(link_text);
                link.block_id = Some(block.id.clone());
                links.push(link);
            }
        }

        // 递归处理子块
        for child in &block.children {
            Self::extract_links_from_block(doc_id, child, links);
        }
    }
}

/// 知识网络引擎
#[derive(Debug, Clone)]
pub struct KnowledgeNetworkEngine {
    link_index: Arc<RwLock<LinkIndex>>,
    graph: Arc<RwLock<KnowledgeGraph>>,
    documents: Arc<RwLock<HashMap<DocId, Document>>>,
}

impl Default for KnowledgeNetworkEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowledgeNetworkEngine {
    pub fn new() -> Self {
        Self {
            link_index: Arc::new(RwLock::new(LinkIndex::new())),
            graph: Arc::new(RwLock::new(KnowledgeGraph::new())),
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册文档到知识网络
    pub fn register_document(&self, doc: Document) {
        let mut index = self.link_index.write();
        index.register_document(doc.id.clone(), doc.title.clone());
        self.documents.write().insert(doc.id.clone(), doc);
    }

    /// 注销文档
    pub fn unregister_document(&self, doc_id: &str) {
        self.link_index.write().unregister_document(doc_id);
        self.documents.write().remove(doc_id);
    }

    /// 索引文档中的所有链接
    pub fn index_document_links(&self, doc: &Document) {
        let extracted = LinkParser::extract_links_from_doc(doc);
        let mut index = self.link_index.write();

        for mut link in extracted {
            // 对于 WikiLink，通过标题解析目标文档ID
            if link.link_type == LinkType::WikiLink {
                if let Some(target_id) = link
                    .anchor_text
                    .as_ref()
                    .and_then(|title| index.resolve_wiki_link(title))
                    .cloned()
                {
                    link.target_doc_id = target_id;
                }
            }

            let target_title = self
                .documents
                .read()
                .get(&link.target_doc_id)
                .map(|d| d.title.clone())
                .unwrap_or_else(|| link.target_doc_id.clone());

            index.add_link(link, target_title);
        }
    }

    /// 重建整个知识图谱
    pub fn rebuild_graph(&self) {
        let mut graph = KnowledgeGraph::new();
        let index = self.link_index.read();
        let docs = self.documents.read();

        // 添加所有节点
        for (doc_id, doc) in docs.iter() {
            let degree = index.get_outgoing_links(doc_id).len() + index.get_backlinks(doc_id).len();
            graph.add_node(GraphNode {
                id: doc_id.clone(),
                title: doc.title.clone(),
                degree,
                cluster: None,
                properties: HashMap::new(),
            });
        }

        // 添加边
        for link in index.all_links() {
            if docs.contains_key(&link.target_doc_id) || docs.contains_key(&link.source_doc_id) {
                graph.add_edge(GraphEdge {
                    id: link.id.clone(),
                    source: link.source_doc_id.clone(),
                    target: link.target_doc_id.clone(),
                    link_type: link.link_type.clone(),
                    semantic_relation: link.semantic_relation.clone(),
                    weight: 1.0,
                });
            }
        }

        *self.graph.write() = graph;
    }

    /// 获取文档的反向链接
    pub fn get_backlinks(&self, doc_id: &str) -> Vec<BacklinkEntry> {
        self.link_index
            .read()
            .get_backlinks(doc_id)
            .into_iter()
            .cloned()
            .collect()
    }

    /// 获取文档的出链
    pub fn get_outgoing_links(&self, doc_id: &str) -> Vec<Link> {
        self.link_index
            .read()
            .get_outgoing_links(doc_id)
            .into_iter()
            .cloned()
            .collect()
    }

    /// 获取知识图谱
    pub fn get_graph(&self) -> KnowledgeGraph {
        self.graph.read().clone()
    }

    /// 获取子图谱（以某节点为中心，指定深度）
    pub fn get_subgraph(&self, center_node_id: &str, depth: usize) -> KnowledgeGraph {
        let graph = self.graph.read();
        let mut subgraph = KnowledgeGraph::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back((center_node_id.to_string(), 0));
        visited.insert(center_node_id.to_string());

        while let Some((node_id, current_depth)) = queue.pop_front() {
            if current_depth > depth {
                continue;
            }

            // 添加节点
            if let Some(node) = graph.nodes.iter().find(|n| n.id == node_id) {
                subgraph.add_node(node.clone());
            }

            if current_depth < depth {
                for edge in graph.outgoing_edges(&node_id) {
                    if !visited.contains(&edge.target) {
                        visited.insert(edge.target.clone());
                        queue.push_back((edge.target.clone(), current_depth + 1));
                    }
                    subgraph.add_edge(edge.clone());
                }

                for edge in graph.incoming_edges(&node_id) {
                    if !visited.contains(&edge.source) {
                        visited.insert(edge.source.clone());
                        queue.push_back((edge.source.clone(), current_depth + 1));
                    }
                    subgraph.add_edge(edge.clone());
                }
            }
        }

        subgraph
    }

    /// BFS 遍历发现可达节点
    pub fn bfs_explore(&self, start_node_id: &str, max_depth: usize) -> Vec<(NodeId, usize)> {
        let graph = self.graph.read();
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();

        queue.push_back((start_node_id.to_string(), 0));
        visited.insert(start_node_id.to_string());
        result.push((start_node_id.to_string(), 0));

        while let Some((node_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            for neighbor_id in graph.neighbors(&node_id) {
                if !visited.contains(&neighbor_id) {
                    visited.insert(neighbor_id.clone());
                    queue.push_back((neighbor_id.clone(), depth + 1));
                    result.push((neighbor_id, depth + 1));
                }
            }
        }

        result
    }

    /// 最短路径发现（BFS）
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<NodeId>> {
        let graph = self.graph.read();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<NodeId, NodeId> = HashMap::new();

        queue.push_back(from.to_string());
        visited.insert(from.to_string());

        while let Some(current) = queue.pop_front() {
            if current == to {
                // 重建路径
                let mut path = vec![to.to_string()];
                let mut node = to.to_string();
                while let Some(p) = parent.get(&node) {
                    path.push(p.clone());
                    node = p.clone();
                }
                path.reverse();
                return Some(path);
            }

            for neighbor in graph.neighbors(&current) {
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor.clone());
                    parent.insert(neighbor.clone(), current.clone());
                    queue.push_back(neighbor);
                }
            }
        }

        None
    }

    /// 添加语义关系链接
    pub fn add_semantic_relation(
        &self,
        source_doc_id: DocId,
        target_doc_id: DocId,
        relation: SemanticRelation,
    ) -> Link {
        let link = Link::new(
            source_doc_id.clone(),
            target_doc_id.clone(),
            LinkType::Relation,
        )
        .with_semantic_relation(relation);

        let mut index = self.link_index.write();
        let target_title = self
            .documents
            .read()
            .get(&target_doc_id)
            .map(|d| d.title.clone())
            .unwrap_or_default();

        index.add_link(link.clone(), target_title);
        link
    }

    /// 获取文档的语义关系
    pub fn get_semantic_relations(&self, doc_id: &str) -> Vec<(Link, SemanticRelation)> {
        self.link_index
            .read()
            .get_outgoing_links(doc_id)
            .iter()
            .filter(|l| l.link_type == LinkType::Relation)
            .filter_map(|l| l.semantic_relation.clone().map(|r| ((*l).clone(), r)))
            .collect()
    }

    /// 更新文档标题（同步更新索引）
    pub fn update_document_title(&self, doc_id: &str, new_title: String) {
        let mut docs = self.documents.write();
        if let Some(doc) = docs.get_mut(doc_id) {
            let old_title = doc.title.clone();
            doc.title = new_title.clone();
            drop(docs);
            self.link_index
                .write()
                .update_title(doc_id, &old_title, new_title);
        }
    }

    /// 搜索文档（通过标题模糊匹配）
    pub fn search_documents(&self, query: &str) -> Vec<Document> {
        let query_lower = query.to_lowercase();
        self.documents
            .read()
            .values()
            .filter(|d| d.title.to_lowercase().contains(&query_lower))
            .cloned()
            .collect()
    }

    /// 获取孤立文档（没有任何链接的文档）
    pub fn get_orphan_documents(&self) -> Vec<Document> {
        let index = self.link_index.read();
        self.documents
            .read()
            .values()
            .filter(|d| {
                index.get_outgoing_links(&d.id).is_empty() && index.get_backlinks(&d.id).is_empty()
            })
            .cloned()
            .collect()
    }

    /// 获取枢纽文档（连接数最多的文档）
    pub fn get_hub_documents(&self, limit: usize) -> Vec<(Document, usize)> {
        let index = self.link_index.read();
        let mut docs: Vec<_> = self
            .documents
            .read()
            .values()
            .map(|d| {
                let degree =
                    index.get_outgoing_links(&d.id).len() + index.get_backlinks(&d.id).len();
                (d.clone(), degree)
            })
            .collect();

        docs.sort_by(|a, b| b.1.cmp(&a.1));
        docs.into_iter().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::content_editor::{Block, ContentEditorEngine};
    use super::*;

    #[test]
    fn test_parse_wiki_links() {
        let text = "See [[Hello World]] and [[Rust|Rust Lang]] for more.";
        let links = LinkParser::parse_wiki_links(text);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, "Hello World");
        assert_eq!(links[0].1, None);
        assert_eq!(links[1].0, "Rust");
        assert_eq!(links[1].1, Some("Rust Lang".to_string()));
    }

    #[test]
    fn test_parse_markdown_links() {
        let text = "Check [Google](https://google.com) and [Rust](https://rust-lang.org).";
        let links = LinkParser::parse_markdown_links(text);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, "Google");
        assert_eq!(links[0].1, "https://google.com");
    }

    #[test]
    fn test_link_index() {
        let mut index = LinkIndex::new();
        index.register_document("doc1".to_string(), "Hello".to_string());
        index.register_document("doc2".to_string(), "World".to_string());

        let link = Link::new("doc1".to_string(), "doc2".to_string(), LinkType::WikiLink);
        index.add_link(link, "World".to_string());

        let outgoing = index.get_outgoing_links("doc1");
        assert_eq!(outgoing.len(), 1);

        let backlinks = index.get_backlinks("doc2");
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_doc_id, "doc1");
    }

    #[test]
    fn test_knowledge_network_engine() {
        let editor = ContentEditorEngine::new();
        let network = KnowledgeNetworkEngine::new();

        // 创建文档1
        let doc1 = editor.create_document("Hello");
        editor.add_block(&doc1.id, Block::text("See [[World]] for more."));
        let doc1 = editor.get_document(&doc1.id).unwrap();
        network.register_document(doc1.clone());

        // 创建文档2
        let doc2 = editor.create_document("World");
        editor.add_block(&doc2.id, Block::text("Back to [[Hello]]."));
        let doc2 = editor.get_document(&doc2.id).unwrap();
        network.register_document(doc2.clone());

        // 索引链接
        network.index_document_links(&doc1);
        network.index_document_links(&doc2);

        // 测试反向链接
        let backlinks = network.get_backlinks(&doc2.id);
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_doc_id, doc1.id);

        // 测试出链
        let outgoing = network.get_outgoing_links(&doc1.id);
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].target_doc_id, doc2.id);
    }

    #[test]
    fn test_graph_build() {
        let editor = ContentEditorEngine::new();
        let network = KnowledgeNetworkEngine::new();

        let doc1 = editor.create_document("A");
        let doc2 = editor.create_document("B");
        let doc3 = editor.create_document("C");

        editor.add_block(&doc1.id, Block::text("Link to [[B]]"));
        editor.add_block(&doc2.id, Block::text("Link to [[C]]"));

        let doc1 = editor.get_document(&doc1.id).unwrap();
        let doc2 = editor.get_document(&doc2.id).unwrap();
        let doc3 = editor.get_document(&doc3.id).unwrap();

        for doc in [&doc1, &doc2, &doc3] {
            network.register_document(doc.clone());
        }

        network.index_document_links(&doc1);
        network.index_document_links(&doc2);
        network.rebuild_graph();

        let graph = network.get_graph();
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);

        // 测试子图谱
        let subgraph = network.get_subgraph(&doc1.id, 1);
        assert_eq!(subgraph.nodes.len(), 2); // A and B
        assert_eq!(subgraph.edges.len(), 1);
    }

    #[test]
    fn test_bfs_explore() {
        let editor = ContentEditorEngine::new();
        let network = KnowledgeNetworkEngine::new();

        let doc1 = editor.create_document("A");
        let doc2 = editor.create_document("B");
        let doc3 = editor.create_document("C");

        editor.add_block(&doc1.id, Block::text("Link to [[B]]"));
        editor.add_block(&doc2.id, Block::text("Link to [[C]]"));

        let doc1 = editor.get_document(&doc1.id).unwrap();
        let doc2 = editor.get_document(&doc2.id).unwrap();
        let doc3 = editor.get_document(&doc3.id).unwrap();

        for doc in [&doc1, &doc2, &doc3] {
            network.register_document(doc.clone());
        }

        network.index_document_links(&doc1);
        network.index_document_links(&doc2);
        network.rebuild_graph();

        let reachable = network.bfs_explore(&doc1.id, 2);
        assert_eq!(reachable.len(), 3); // A(0), B(1), C(2)
    }

    #[test]
    fn test_shortest_path() {
        let editor = ContentEditorEngine::new();
        let network = KnowledgeNetworkEngine::new();

        let doc1 = editor.create_document("A");
        let doc2 = editor.create_document("B");
        let doc3 = editor.create_document("C");

        editor.add_block(&doc1.id, Block::text("Link to [[B]]"));
        editor.add_block(&doc2.id, Block::text("Link to [[C]]"));

        let doc1 = editor.get_document(&doc1.id).unwrap();
        let doc2 = editor.get_document(&doc2.id).unwrap();
        let doc3 = editor.get_document(&doc3.id).unwrap();

        for doc in [&doc1, &doc2, &doc3] {
            network.register_document(doc.clone());
        }

        network.index_document_links(&doc1);
        network.index_document_links(&doc2);
        network.rebuild_graph();

        let path = network.shortest_path(&doc1.id, &doc3.id);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], doc1.id);
        assert_eq!(path[2], doc3.id);
    }

    #[test]
    fn test_semantic_relation() {
        let editor = ContentEditorEngine::new();
        let network = KnowledgeNetworkEngine::new();

        let doc1 = editor.create_document("Thesis");
        let doc2 = editor.create_document("Evidence");

        network.register_document(doc1.clone());
        network.register_document(doc2.clone());

        let link = network.add_semantic_relation(
            doc1.id.clone(),
            doc2.id.clone(),
            SemanticRelation::Supports,
        );

        assert_eq!(link.link_type, LinkType::Relation);
        assert_eq!(link.semantic_relation, Some(SemanticRelation::Supports));

        let relations = network.get_semantic_relations(&doc1.id);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].1, SemanticRelation::Supports);
    }

    #[test]
    fn test_orphan_and_hub() {
        let editor = ContentEditorEngine::new();
        let network = KnowledgeNetworkEngine::new();

        let doc1 = editor.create_document("Hub");
        let doc2 = editor.create_document("Link1");
        let doc3 = editor.create_document("Orphan");

        editor.add_block(&doc1.id, Block::text("See [[Link1]] and more."));
        editor.add_block(&doc2.id, Block::text("Back to [[Hub]]."));

        let doc1 = editor.get_document(&doc1.id).unwrap();
        let doc2 = editor.get_document(&doc2.id).unwrap();
        let doc3 = editor.get_document(&doc3.id).unwrap();

        for doc in [&doc1, &doc2, &doc3] {
            network.register_document(doc.clone());
        }

        network.index_document_links(&doc1);
        network.index_document_links(&doc2);

        let orphans = network.get_orphan_documents();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].id, doc3.id);

        let hubs = network.get_hub_documents(2);
        assert_eq!(hubs.len(), 2);
        // Hub 和 Link1 都有 2 个连接，验证两者都在列表中
        let hub_ids: HashSet<_> = hubs.iter().map(|h| h.0.id.clone()).collect();
        assert!(hub_ids.contains(&doc1.id));
        assert!(hub_ids.contains(&doc2.id));
    }

    #[test]
    fn test_update_title() {
        let editor = ContentEditorEngine::new();
        let network = KnowledgeNetworkEngine::new();

        let doc = editor.create_document("Old Title");
        network.register_document(doc.clone());

        assert!(network
            .link_index
            .read()
            .resolve_wiki_link("Old Title")
            .is_some());

        network.update_document_title(&doc.id, "New Title".to_string());

        assert!(network
            .link_index
            .read()
            .resolve_wiki_link("Old Title")
            .is_none());
        assert!(network
            .link_index
            .read()
            .resolve_wiki_link("New Title")
            .is_some());
    }
}
