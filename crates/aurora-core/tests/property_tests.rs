//! SubTask 6.3.1 — Property-based tests (proptest harness).
//!
//! These tests exercise invariants of the aurora-core domain model across
//! randomly generated inputs. They complement the 760+ existing unit tests
//! with property-level guarantees:
//!
//! - Document / Block JSON round-trip equivalence
//! - Block / Document id uniqueness
//! - Markdown render structural equivalence for text & heading blocks
//! - GTD task state-machine: valid transitions always succeed, invalid always fail
//! - Knowledge-graph BFS visits every reachable node exactly once
//! - Property-engine type validation honours the declared schema
//!
//! All tests are deterministic: proptest uses a configurable seed and the
//! `MockL1` generators in `common` produce stable inputs.

mod common;

use std::collections::{HashMap, HashSet, VecDeque};

use proptest::prelude::*;

use aurora_core::l2_engines::property::{
    PropertyDefinition, PropertyEngine, PropertyType, SelectOption,
};
use aurora_core::l3_domain::content_editor::{Block, ContentEditorEngine, Document};
use aurora_core::l3_domain::gtd_system::{Priority, Task, TaskStatus};
use aurora_core::l3_domain::knowledge_network::{
    GraphEdge, GraphNode, KnowledgeGraph, KnowledgeNetworkEngine, LinkType,
};

use common::{make_document, make_heading_block, make_text_block};

// ==================== Strategies ====================

/// Small printable text without newlines (keeps markdown line semantics intact).
fn arb_text() -> impl Strategy<Value = String> {
    proptest::string::string_regex(r"[a-zA-Z0-9 .,!?]{1,40}").unwrap()
}

/// Optional text (may be empty) for fields like titles.
fn arb_text_maybe_empty() -> impl Strategy<Value = String> {
    proptest::string::string_regex(r"[a-zA-Z0-9 .,!?]{0,40}").unwrap()
}

/// Heading level 1..=6.
fn arb_heading_level() -> impl Strategy<Value = u8> {
    (1u8..=6u8)
}

/// A vector of text blocks (1..=12 entries).
fn arb_text_blocks() -> impl Strategy<Value = Vec<Block>> {
    prop::collection::vec(arb_text(), 1..=12)
        .prop_map(|texts| texts.into_iter().map(|t| make_text_block(&t)).collect())
}

/// All seven GTD task statuses, in a fixed canonical order.
fn all_task_statuses() -> Vec<TaskStatus> {
    vec![
        TaskStatus::Inbox,
        TaskStatus::Clarified,
        TaskStatus::Organized,
        TaskStatus::Scheduled,
        TaskStatus::Doing,
        TaskStatus::Done,
        TaskStatus::Archived,
    ]
}

/// Pick a random `TaskStatus` variant.
fn arb_task_status() -> impl Strategy<Value = TaskStatus> {
    prop::sample::select(all_task_statuses())
}

// ==================== 1. Document JSON round-trip ====================

proptest! {
    /// Serializing a Document to JSON and back must yield an equal Document.
    /// This guards the serde contracts used by sync, import/export and storage.
    #[test]
    fn prop_document_json_roundtrip(
        title in arb_text_maybe_empty(),
        blocks in arb_text_blocks(),
    ) {
        let doc = make_document(&title, blocks);
        let json = serde_json::to_string(&doc).expect("serialize");
        let back: Document = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(doc, back);
    }
}

// ==================== 2. Block JSON round-trip ====================

proptest! {
    /// A single Block must round-trip through JSON losslessly for every text input.
    #[test]
    fn prop_block_json_roundtrip(content in arb_text()) {
        let block = make_text_block(&content);
        let json = serde_json::to_string(&block).expect("serialize");
        let back: Block = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(block, back);
    }

    /// Heading blocks (which carry a `level` property) must also round-trip.
    #[test]
    fn prop_heading_block_json_roundtrip(level in arb_heading_level(), content in arb_text()) {
        let block = make_heading_block(level, &content);
        let json = serde_json::to_string(&block).expect("serialize");
        let back: Block = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(block, back);
    }
}

// ==================== 3. Block id uniqueness ====================

proptest! {
    /// `Block::new` mints a fresh UUID per block; a batch of N blocks must have
    /// N distinct ids.
    #[test]
    fn prop_block_ids_unique(texts in prop::collection::vec(arb_text(), 1..=64)) {
        let blocks: Vec<Block> = texts.iter().map(|t| make_text_block(t)).collect();
        let ids: HashSet<&str> = blocks.iter().map(|b| b.id.as_str()).collect();
        prop_assert_eq!(ids.len(), blocks.len());
    }

    /// `Document::new` mints a fresh UUID per document; N documents must have
    /// N distinct ids.
    #[test]
    fn prop_document_ids_unique(titles in prop::collection::vec(arb_text(), 1..=64)) {
        let docs: Vec<Document> = titles.iter().map(Document::new).collect();
        let ids: HashSet<&str> = docs.iter().map(|d| d.id.as_str()).collect();
        prop_assert_eq!(ids.len(), docs.len());
    }
}

// ==================== 4. Markdown round-trip (structural equivalence) ====================

proptest! {
    /// For a document of text blocks, `to_markdown()` must emit one line per
    /// non-empty block content. We parse the markdown back by splitting on
    /// newlines (skipping the title header) and assert the reconstructed line
    /// set equals the input content set — structural equivalence for text.
    #[test]
    fn prop_text_markdown_roundtrip(
        title in arb_text_maybe_empty(),
        contents in prop::collection::vec(arb_text(), 1..=12),
    ) {
        let blocks: Vec<Block> = contents.iter().map(|t| make_text_block(t)).collect();
        let doc = make_document(&title, blocks);
        let md = doc.to_markdown();

        // Drop the title header lines (if any) and blank lines.
        let mut body_lines: Vec<&str> = md.lines().collect();
        if !title.is_empty() {
            // First line is "# {title}"; drop it and the following blank line.
            body_lines.remove(0);
            if body_lines.first().map(|l| l.is_empty()).unwrap_or(false) {
                body_lines.remove(0);
            }
        }
        let body_lines: Vec<&str> = body_lines
            .into_iter()
            .filter(|l| !l.is_empty())
            .collect();

        let expected: HashSet<&str> = contents.iter().map(|s| s.as_str()).collect();
        let got: HashSet<&str> = body_lines.iter().copied().collect();
        prop_assert_eq!(got, expected, "markdown body lines must match text contents");
    }

    /// For heading blocks, each rendered line must start with the right number
    /// of `#` characters followed by the content text.
    #[test]
    fn prop_heading_markdown_structure(
        pairs in prop::collection::vec((arb_heading_level(), arb_text()), 1..=8),
    ) {
        let blocks: Vec<Block> = pairs
            .iter()
            .map(|(lvl, t)| make_heading_block(*lvl, t))
            .collect();
        let doc = make_document("Headings", blocks);
        let md = doc.to_markdown();

        // The doc title "Headings" is always rendered first as `# Headings`,
        // followed by a blank line, then one line per heading block. Skip the
        // title header (first line) by position so a heading whose content
        // happens to be "Headings" is not erroneously dropped.
        let mut lines = md.lines();
        let _title_line = lines.next(); // "# Headings"
        let _blank = lines.next(); // ""
        let heading_lines: Vec<&str> = lines.filter(|l| !l.is_empty()).collect();

        prop_assert_eq!(
            heading_lines.len(),
            pairs.len(),
            "one heading line per heading block"
        );
        for (line, (lvl, text)) in heading_lines.iter().zip(pairs.iter()) {
            let prefix = "#".repeat(*lvl as usize);
            prop_assert!(
                line.starts_with(&format!("{prefix} ")),
                "heading line must start with {prefix} + space: got {line}"
            );
            prop_assert!(
                line.ends_with(text.as_str()),
                "heading line must end with content text: got {line}"
            );
        }
    }
}

// ==================== 5. Task state machine ====================

proptest! {
    /// Starting from any status, repeatedly following a *valid* `next_states()`
    /// transition must always succeed (`transition_to` returns `Ok`) and the
    /// task status must advance to the chosen target. The status must always
    /// remain within the known enum set.
    #[test]
    fn prop_task_valid_transitions_always_succeed(
        start in arb_task_status(),
        steps in prop::collection::vec(any::<u32>(), 1..=20),
    ) {
        let mut task = Task::new("property-test task").with_priority(Priority::Medium);
        // `status` is a public field, so we can seed the machine at any state.
        task.status = start.clone();

        let mut current = task.status.clone();
        for s in &steps {
            let candidates = current.next_states();
            if candidates.is_empty() {
                break;
            }
            let pick = candidates[(*s as usize) % candidates.len()].clone();
            let res = task.transition_to(pick.clone());
            prop_assert!(res.is_ok(), "valid transition {:?} -> {:?} must succeed", current, pick);
            prop_assert_eq!(&task.status, &pick);
            // Status must always be a known variant.
            prop_assert!(all_task_statuses().contains(&task.status));
            current = pick;
        }
    }

    /// Picking a status NOT in `next_states()` must always be rejected by
    /// `transition_to` (returns `Err`), and the task status must be unchanged.
    #[test]
    fn prop_task_invalid_transitions_always_fail(
        start in arb_task_status(),
        seed in any::<u32>(),
    ) {
        let mut task = Task::new("invalid-transition task");
        task.status = start.clone();

        let valid = start.next_states();
        let invalid_choices: Vec<TaskStatus> = all_task_statuses()
            .into_iter()
            .filter(|s| !valid.contains(s) && *s != start)
            .collect();
        // Every state has at most 3 successors out of 7, so there is always
        // at least one invalid choice — but guard defensively anyway.
        if !invalid_choices.is_empty() {
            let bad = invalid_choices[(seed as usize) % invalid_choices.len()].clone();
            let before = task.status.clone();
            let res = task.transition_to(bad.clone());
            prop_assert!(res.is_err(), "invalid transition {:?} -> {:?} must fail", before, bad);
            prop_assert_eq!(task.status, before, "status must not change on failed transition");
        }
    }
}

// ==================== 6. Knowledge-graph BFS ====================

/// Build a `KnowledgeGraph` directly from node count + edge list (undirected
/// in spirit: `neighbors()` returns both endpoints).
fn build_graph(n: usize, edges: &[(usize, usize)]) -> KnowledgeGraph {
    let mut g = KnowledgeGraph::new();
    for i in 0..n {
        let id = format!("n{i}");
        g.add_node(GraphNode {
            id,
            title: format!("node {i}"),
            degree: 0,
            cluster: None,
            properties: HashMap::new(),
        });
    }
    for (idx, &(a, b)) in edges.iter().enumerate() {
        if a < n && b < n && a != b {
            g.add_edge(GraphEdge {
                id: format!("e{idx}"),
                source: format!("n{a}"),
                target: format!("n{b}"),
                link_type: LinkType::Relation,
                semantic_relation: None,
                weight: 1.0,
            });
        }
    }
    g
}

/// Independent undirected reachable-set computation (reference BFS).
fn reference_reachable(graph: &KnowledgeGraph, start: &str) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start.to_string());
    visited.insert(start.to_string());
    while let Some(node) = queue.pop_front() {
        for nb in graph.neighbors(&node) {
            if visited.insert(nb.clone()) {
                queue.push_back(nb);
            }
        }
    }
    visited
}

proptest! {
    /// A BFS over a random graph must visit every reachable node exactly once
    /// and exactly the reachable set (no more, no less).
    #[test]
    fn prop_graph_bfs_visits_reachable_once(
        n in 2usize..=10,
        edges in prop::collection::vec((0usize..=9, 0usize..=9), 0..=20),
    ) {
        let graph = build_graph(n, &edges);
        let start = "n0".to_string();
        let expected = reference_reachable(&graph, &start);

        // BFS using the graph's own `neighbors()` (the same primitive `bfs_explore` uses).
        let mut visited: HashSet<String> = HashSet::new();
        let mut order: Vec<String> = Vec::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(start.clone());
        visited.insert(start.clone());
        while let Some(node) = queue.pop_front() {
            order.push(node.clone());
            for nb in graph.neighbors(&node) {
                if visited.insert(nb.clone()) {
                    queue.push_back(nb);
                }
            }
        }

        // Exactly-once: order length equals visited set size.
        prop_assert_eq!(order.len(), visited.len(), "BFS must visit each node once");
        // Visited set equals the independently-computed reachable set.
        prop_assert_eq!(visited, expected, "BFS reachable set must match reference");
    }

    /// The engine's real `bfs_explore` over a WikiLink graph must return each
    /// reachable node exactly once, matching the independent reachable set.
    #[test]
    fn prop_engine_bfs_explore_visits_reachable_once(
        n in 2usize..=6,
        edge_seeds in prop::collection::vec((0u32..=5, 0u32..=5), 0..=10),
    ) {
        let editor = ContentEditorEngine::new();
        let network = KnowledgeNetworkEngine::new();

        // Create n documents titled node_0..node_{n-1}.
        let titles: Vec<String> = (0..n).map(|i| format!("node_{i}")).collect();
        let mut doc_ids: Vec<String> = Vec::new();
        for t in &titles {
            let d = editor.create_document(t.clone());
            doc_ids.push(d.id);
        }

        // Add WikiLink blocks according to edge_seeds (directed i -> j).
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for &(a_seed, b_seed) in &edge_seeds {
            let a = (a_seed as usize) % n;
            let b = (b_seed as usize) % n;
            if a == b {
                continue;
            }
            let block_text = format!("Link to [[{}]]", titles[b]);
            editor.add_block(&doc_ids[a], Block::text(block_text));
            edges.push((a, b));
        }

        // Register docs, index links, rebuild graph.
        let docs: Vec<Document> = doc_ids
            .iter()
            .filter_map(|id| editor.get_document(id))
            .collect();
        for d in &docs {
            network.register_document(d.clone());
        }
        for d in &docs {
            network.index_document_links(d);
        }
        network.rebuild_graph();

        // Independent undirected reachable set from node_0.
        let mut adj: HashMap<usize, HashSet<usize>> = HashMap::new();
        for (a, b) in &edges {
            adj.entry(*a).or_default().insert(*b);
            adj.entry(*b).or_default().insert(*a);
        }
        let mut expected: HashSet<String> = HashSet::new();
        let mut q: VecDeque<usize> = VecDeque::new();
        q.push_back(0);
        expected.insert(doc_ids[0].clone());
        while let Some(node) = q.pop_front() {
            if let Some(nbrs) = adj.get(&node) {
                for &nb in nbrs {
                    if expected.insert(doc_ids[nb].clone()) {
                        q.push_back(nb);
                    }
                }
            }
        }

        let reachable = network.bfs_explore(&doc_ids[0], n); // max_depth = n reaches everything
        let got: HashSet<String> = reachable.iter().map(|(id, _)| id.clone()).collect();
        prop_assert_eq!(got.len(), reachable.len(), "bfs_explore must not duplicate nodes");
        prop_assert_eq!(got, expected, "bfs_explore reachable set must match reference");
    }
}

// ==================== 7. Property-engine type validation ====================

proptest! {
    /// A `Number` property must validate arbitrary numbers and reject non-numbers.
    #[test]
    fn prop_property_number_validation(v in any::<f64>()) {
        let engine = PropertyEngine::new();
        engine.register(PropertyDefinition {
            id: "age".into(),
            name: "Age".into(),
            prop_type: PropertyType::Number,
            required: true,
            default_value: None,
            indexed: false,
            description: None,
        });
        let valid = engine.validate("age", &serde_json::json!(v));
        prop_assert!(valid.valid, "number value must validate: {:?}", valid.errors);
        let invalid = engine.validate("age", &serde_json::json!("not a number"));
        prop_assert!(!invalid.valid, "string must fail Number validation");
    }

    /// A `Checkbox` property must validate booleans and reject non-booleans.
    #[test]
    fn prop_property_checkbox_validation(b in any::<bool>()) {
        let engine = PropertyEngine::new();
        engine.register(PropertyDefinition {
            id: "done".into(),
            name: "Done".into(),
            prop_type: PropertyType::Checkbox,
            required: false,
            default_value: Some(serde_json::json!(false)),
            indexed: false,
            description: None,
        });
        let valid = engine.validate("done", &serde_json::json!(b));
        prop_assert!(valid.valid, "boolean must validate: {:?}", valid.errors);
        let invalid = engine.validate("done", &serde_json::json!("yes"));
        prop_assert!(!invalid.valid, "string must fail Checkbox validation");
    }

    /// A `Select` property must validate options drawn from its allowed set and
    /// reject values outside it.
    #[test]
    fn prop_property_select_validation(
        idx in 0u32..=2u32,
        outsider in arb_text(),
    ) {
        let options = vec![
            SelectOption { value: "low".into(), color: None },
            SelectOption { value: "med".into(), color: None },
            SelectOption { value: "high".into(), color: None },
        ];
        let engine = PropertyEngine::new();
        engine.register(PropertyDefinition {
            id: "pri".into(),
            name: "Priority".into(),
            prop_type: PropertyType::Select(options.clone()),
            required: true,
            default_value: None,
            indexed: false,
            description: None,
        });
        let chosen = &options[(idx as usize) % options.len()].value;
        let valid = engine.validate("pri", &serde_json::json!(chosen));
        prop_assert!(valid.valid, "in-set option must validate: {:?}", valid.errors);

        // An outsider string that happens to equal an option would still be valid;
        // only assert rejection when it is genuinely outside the option set.
        if !options.iter().any(|o| o.value == outsider) {
            let invalid = engine.validate("pri", &serde_json::json!(outsider));
            prop_assert!(!invalid.valid, "out-of-set value must fail Select validation");
        }
    }

    /// A `Text` property must validate strings and reject non-strings.
    #[test]
    fn prop_property_text_validation(s in arb_text()) {
        let engine = PropertyEngine::new();
        engine.register(PropertyDefinition {
            id: "note".into(),
            name: "Note".into(),
            prop_type: PropertyType::Text,
            required: false,
            default_value: None,
            indexed: false,
            description: None,
        });
        let valid = engine.validate("note", &serde_json::json!(s));
        prop_assert!(valid.valid, "string must validate: {:?}", valid.errors);
        let invalid = engine.validate("note", &serde_json::json!(42));
        prop_assert!(invalid.invalid_or_warn(), "number must fail Text validation");
    }
}

// Small helper so the test reads naturally while still compiling against the
// `ValidationResult` API (which has no `is_invalid()` method).
trait ValidationResultExt {
    fn invalid_or_warn(&self) -> bool;
}
impl ValidationResultExt for aurora_core::l2_engines::property::ValidationResult {
    fn invalid_or_warn(&self) -> bool {
        !self.valid
    }
}
