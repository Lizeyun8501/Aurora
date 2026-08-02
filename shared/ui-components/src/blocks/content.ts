/**
 * Internal helpers for safely extracting typed values from a block's free-form
 * `content` / `properties` JSON. Block content shape depends on `block_type`;
 * these accessors degrade gracefully to defaults when fields are absent.
 */
import type { Block, JsonObject, JsonValue } from '@aurora/shared-types';

/** Props shared by every block renderer component. */
export interface BlockComponentProps {
  block: Block;
  className?: string;
}

/** Returns the block's `content` as a JSON object (empty object if not an object). */
export function blockContent(block: Block): JsonObject {
  const c = block.content;
  if (c !== null && typeof c === 'object' && !Array.isArray(c)) {
    return c as JsonObject;
  }
  return {};
}

/** Returns the block's text — either the raw string content or `content.text`. */
export function blockText(block: Block): string {
  const c = block.content;
  if (typeof c === 'string') return c;
  return asString(blockContent(block).text);
}

export function asString(v: JsonValue | undefined): string {
  return typeof v === 'string' ? v : '';
}

export function asNumber(v: JsonValue | undefined): number {
  return typeof v === 'number' && Number.isFinite(v) ? v : 0;
}

export function asBoolean(v: JsonValue | undefined): boolean {
  return typeof v === 'boolean' ? v : false;
}

export function asNullableString(v: JsonValue | undefined): string | null {
  return typeof v === 'string' ? v : null;
}

export function asStringArray(v: JsonValue | undefined): string[] {
  if (Array.isArray(v)) {
    return v.filter((x): x is string => typeof x === 'string');
  }
  return [];
}

export function asStringMatrix(v: JsonValue | undefined): string[][] {
  if (Array.isArray(v)) {
    return v
      .filter((row): row is JsonValue[] => Array.isArray(row))
      .map((row) => row.filter((cell): cell is string => typeof cell === 'string'));
  }
  return [];
}
