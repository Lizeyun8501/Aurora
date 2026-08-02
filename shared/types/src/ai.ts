/**
 * AI intelligence system domain types.
 * Mirrors `crates/aurora-core/src/l3_domain/ai_system.rs` and
 * `crates/aurora-core/src/traits/ai_provider.rs`.
 */

/** A vector embedding (mirrors `Vec<f32>`). */
export type Embedding = number[];

/** Chat message role. */
export type ChatRole = 'system' | 'user' | 'assistant' | 'tool';

/** A chat message (mirrors `Message`). */
export interface ChatMessage {
  role: ChatRole;
  content: string;
}

/** Completion options (mirrors `CompletionOptions`). */
export interface CompletionOptions {
  max_tokens: number | null;
  temperature: number | null;
  top_p: number | null;
  stop: string[] | null;
}

/** Chat options (mirrors `ChatOptions`). */
export interface ChatOptions {
  max_tokens: number | null;
  temperature: number | null;
}

/** A tool/function definition (mirrors `Tool`). */
export interface Tool {
  name: string;
  description: string;
  parameters: unknown;
}

/** A tool call request (mirrors `ToolCall`). */
export interface ToolCall {
  tool_name: string;
  arguments: unknown;
}

/** Inference / provider routing strategy (mirrors `InferenceStrategy`). */
export type AIProviderStrategy = 'local_first' | 'cloud_only' | 'auto';

/** Kind of AI request, used to discriminate `AIRequest`. */
export type AIRequestKind =
  | 'completion'
  | 'chat'
  | 'embed'
  | 'function_call'
  | 'summary'
  | 'autotag'
  | 'task_decomposition';

/** An AI request (frontend-facing union over the AIProvider operations). */
export type AIRequest =
  | { kind: 'completion'; prompt: string; options: CompletionOptions }
  | { kind: 'chat'; messages: ChatMessage[]; options: ChatOptions }
  | { kind: 'embed'; texts: string[] }
  | { kind: 'function_call'; prompt: string; tools: Tool[] }
  | { kind: 'summary'; doc_id: string; content: string }
  | { kind: 'autotag'; content: string }
  | { kind: 'task_decomposition'; task_title: string; task_description: string };

/** An AI response (frontend-facing). */
export interface AIResponse {
  request_id: string;
  request_kind: AIRequestKind;
  /** Generated text (completion / chat / summary). */
  output: string;
  /** Embedding vectors for `embed` requests. */
  embeddings: Embedding[];
  /** Tool call result for `function_call` requests. */
  tool_call: ToolCall | null;
  /** Suggested tags for `autotag` requests. */
  tags: string[];
  /** Sub-task titles for `task_decomposition` requests. */
  subtasks: string[];
  /** Provider that served the request (`local` | `cloud`). */
  served_by: 'local' | 'cloud';
  elapsed_ms: number;
}

/**
 * AI provider capability surface (mirrors the `AIProvider` trait).
 * Implementations may be local (on-device) or cloud.
 */
export interface AIProvider {
  readonly id: string;
  readonly kind: 'local' | 'cloud';
  isAvailable(): boolean;
  embed(texts: string[]): Promise<Embedding[]>;
  complete(prompt: string, options: CompletionOptions): Promise<string>;
  chat(messages: ChatMessage[], options: ChatOptions): Promise<string>;
  functionCall(prompt: string, tools: Tool[]): Promise<ToolCall>;
}

/** Hybrid search result item (full-text + vector). */
export interface SearchResult {
  doc_id: string;
  title: string;
  score: number;
  snippet: string | null;
  /** Source of the match. */
  matched_by: 'fulltext' | 'vector' | 'hybrid';
}

/** Reciprocal Rank Fusion (RRF) result. */
export interface RrfResult {
  doc_id: string;
  /** Fused score (higher is better). */
  score: number;
  /** Contributing rank lists. */
  sources: string[];
}

/** Content summarization result. */
export interface SummaryResult {
  doc_id: string;
  summary: string;
  key_points: string[];
  estimated_reading_time_minutes: number;
}
