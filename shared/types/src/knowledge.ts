/**
 * Knowledge Network domain types.
 * Mirrors `crates/aurora-core/src/l3_domain/knowledge_network.rs`.
 */

import type { JsonObject } from './blocks';

/** Link unique identifier (mirrors `LinkId`). */
export type LinkId = string;

/** Node unique identifier — corresponds to a `DocId` (mirrors `NodeId`). */
export type NodeId = string;

/** Link type (mirrors `LinkType`, serde `snake_case`). */
export type LinkType = 'wiki_link' | 'markdown_link' | 'relation';

/**
 * Semantic relation label (mirrors `SemanticRelation`).
 * `custom:<name>` represents the Rust `SemanticRelation::Custom(String)` variant.
 */
export type RelationType =
  | 'supports'
  | 'refutes'
  | 'references'
  | 'extends'
  | 'related'
  | `custom:${string}`;

/**
 * Alias kept for clarity — same shape as `RelationType`.
 * Mirrors the full `SemanticRelation` enum.
 */
export type SemanticRelation = RelationType;

/** A parsed `[[wiki link]]` (optionally with display text `[[target|display]]`). */
export interface WikiLink {
  target: string;
  display_text: string | null;
}

/** A link between two documents (mirrors `Link`). */
export interface Link {
  id: LinkId;
  source_doc_id: string;
  target_doc_id: string;
  link_type: LinkType;
  semantic_relation: RelationType | null;
  anchor_text: string | null;
  block_id: string | null;
  created_at: string;
}

/** A backlink entry with context preview (mirrors `BacklinkEntry`). */
export interface Backlink {
  source_doc_id: string;
  source_doc_title: string;
  link_id: LinkId;
  anchor_text: string | null;
  block_id: string | null;
  semantic_relation: RelationType | null;
}

/** Graph node (mirrors `GraphNode`). */
export interface GraphNode {
  id: NodeId;
  title: string;
  /** Degree (number of connections). */
  degree: number;
  /** Cluster label, if clustered. */
  cluster: string | null;
  /** Custom properties. */
  properties: JsonObject;
}

/** Graph edge (mirrors `GraphEdge`). */
export interface GraphEdge {
  id: string;
  source: NodeId;
  target: NodeId;
  link_type: LinkType;
  semantic_relation: RelationType | null;
  weight: number;
}

/** Knowledge graph (mirrors `KnowledgeGraph`). */
export interface KnowledgeGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}
