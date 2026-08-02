/**
 * Content Editor domain types.
 * Mirrors `crates/aurora-core/src/l3_domain/content_editor.rs`.
 *
 * Pure type definitions — no runtime logic.
 */

/** ISO-8601 UTC date-time string (e.g. "2024-06-01T00:00:00Z"). */
export type ISODateString = string;

/** JSON value (mirrors `serde_json::Value`). */
export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

/** A JSON object map (mirrors `HashMap<String, serde_json::Value>`). */
export type JsonObject = Record<string, JsonValue>;

/** Document unique identifier (mirrors `DocId`). */
export type DocId = string;

/** Block unique identifier (mirrors `BlockId`). */
export type BlockId = string;

/**
 * Block type enumeration.
 *
 * Built-in variants mirror the Rust `BlockType` enum (serde `snake_case`).
 * The `(string & {})` term keeps the union extensible so custom block types
 * (Rust `BlockType::Custom(String)`, serialized as `"custom:<name>"`) are
 * assignable while still providing literal autocomplete.
 */
export type BlockType =
  | 'text'
  | 'heading'
  | 'code'
  | 'image'
  | 'table'
  | 'divider'
  | 'quote'
  | 'list_item'
  | 'todo_item'
  | (string & {});

/** Inline text mark types (Tiptap-style inline marks). */
export type MarkType =
  | 'bold'
  | 'italic'
  | 'underline'
  | 'strikethrough'
  | 'code'
  | 'link'
  | 'highlight'
  | (string & {});

/** An inline text mark applied to a range within a block's text content. */
export interface TextMark {
  type: MarkType;
  start: number;
  end: number;
  attrs?: JsonObject;
}

/** A block's property bag (mirrors `HashMap<String, serde_json::Value>`). */
export type BlockProperties = Record<string, JsonValue>;

/**
 * Block structure (mirrors `Block`).
 * `content` is a free-form JSON value whose shape depends on `block_type`.
 */
export interface Block {
  id: BlockId;
  block_type: BlockType;
  content: JsonValue;
  properties: BlockProperties;
  children: Block[];
  created_at: ISODateString;
  updated_at: ISODateString;
}

/** Document structure (mirrors `Document`). */
export interface Document {
  id: DocId;
  title: string;
  blocks: Block[];
  properties: BlockProperties;
  created_at: ISODateString;
  updated_at: ISODateString;
  version: number;
}

/** Block type definition (mirrors `BlockTypeDef`). */
export interface BlockTypeDef {
  name: string;
  display_name: string;
  icon: string;
  schema: JsonValue;
  default_props: BlockProperties;
}

/** Comment anchor (mirrors `CommentAnchor`). */
export type CommentAnchor =
  | { kind: 'document' }
  | { kind: 'block' }
  | { kind: 'text_range'; start: number; end: number };

/** A comment reply (mirrors `CommentReply`). */
export interface CommentReply {
  id: string;
  author_id: string;
  content: string;
  created_at: ISODateString;
}

/** A comment / annotation (mirrors `Comment`). */
export interface Comment {
  id: string;
  doc_id: DocId;
  block_id: BlockId | null;
  anchor: CommentAnchor;
  author_id: string;
  content: string;
  created_at: ISODateString;
  resolved: boolean;
  replies: CommentReply[];
}

/** Document version-history snapshot (mirrors `DocumentSnapshot`). */
export interface DocumentSnapshot {
  id: string;
  doc_id: DocId;
  version: number;
  document: Document;
  created_at: ISODateString;
  created_by: string;
  comment: string | null;
}
