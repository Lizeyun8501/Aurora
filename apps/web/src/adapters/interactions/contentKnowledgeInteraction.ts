/**
 * SubTask 5.4.1 — Content Editor ↔ Knowledge Network interaction.
 *
 * The {@link ContentKnowledgeController} listens for `BlockChanged` events on
 * the {@link EventBus}, parses `[[wiki links]]` and `[markdown](links)` out of
 * the block content, reconciles the link index (mock mutation), and emits
 * `BacklinksUpdated` so the Knowledge Network graph can refresh.
 *
 * The hook {@link useContentKnowledgeLink} exposes the latest parsed links to
 * React components and binds the controller lifecycle to the component tree.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import type {
  CoreEvent,
  JsonValue,
  Link,
  LinkType,
  WikiLink,
} from '@aurora/shared-types';
import { EventBus, type Subscription } from './eventBus';

/** A minimal Knowledge Network link-index store surface. */
export interface KnowledgeLinkStore {
  /** Replace all links whose source is `docId`. */
  setLinksForDoc(docId: string, links: Link[]): void;
  /** Read all links whose source is `docId`. */
  getLinksForDoc(docId: string): Link[];
}

/** A minimal in-memory {@link KnowledgeLinkStore} implementation. */
export class InMemoryKnowledgeLinkStore implements KnowledgeLinkStore {
  private readonly byDoc = new Map<string, Link[]>();

  setLinksForDoc(docId: string, links: Link[]): void {
    this.byDoc.set(docId, links);
  }

  getLinksForDoc(docId: string): Link[] {
    return this.byDoc.get(docId) ?? [];
  }
}

/** Parsed markdown link `[text](target)`. */
export interface MarkdownLink {
  text: string;
  target: string;
}

/** Result of parsing a block's text content for outgoing links. */
export interface ParsedLinks {
  wikiLinks: WikiLink[];
  markdownLinks: MarkdownLink[];
}

/**
 * Parse `[[wiki]]` / `[[wiki|display]]` links from `text`.
 */
export function parseWikiLinks(text: string): WikiLink[] {
  const re = /\[\[([^\]]+)\]\]/g;
  const out: WikiLink[] = [];
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    const inner = match[1];
    const pipe = inner.indexOf('|');
    if (pipe >= 0) {
      out.push({
        target: inner.slice(0, pipe).trim(),
        display_text: inner.slice(pipe + 1).trim() || null,
      });
    } else {
      out.push({ target: inner.trim(), display_text: null });
    }
  }
  return out;
}

/**
 * Parse `[text](target)` markdown links from `text`. Image links
 * `![alt](src)` are intentionally excluded (they are assets, not docs).
 */
export function parseMarkdownLinks(text: string): MarkdownLink[] {
  const re = /(?<!!)\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;
  const out: MarkdownLink[] = [];
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    out.push({ text: match[1], target: match[2] });
  }
  return out;
}

/** Coerce a {@link JsonValue} block content to a parseable string. */
function contentToString(content: JsonValue): string {
  if (typeof content === 'string') return content;
  if (content == null) return '';
  try {
    return JSON.stringify(content);
  } catch {
    return '';
  }
}

/** Convert parsed links to {@link Link} records for a given source doc/block. */
function toLinks(
  docId: string,
  blockId: string,
  parsed: ParsedLinks,
  createdAt: string,
): Link[] {
  const links: Link[] = [];
  for (const w of parsed.wikiLinks) {
    links.push({
      id: `${blockId}:wiki:${w.target}`,
      source_doc_id: docId,
      target_doc_id: w.target,
      link_type: 'wiki_link' satisfies LinkType,
      semantic_relation: null,
      anchor_text: w.display_text,
      block_id: blockId,
      created_at: createdAt,
    });
  }
  for (const m of parsed.markdownLinks) {
    links.push({
      id: `${blockId}:md:${m.target}`,
      source_doc_id: docId,
      target_doc_id: m.target,
      link_type: 'markdown_link' satisfies LinkType,
      semantic_relation: null,
      anchor_text: m.text,
      block_id: blockId,
      created_at: createdAt,
    });
  }
  return links;
}

export interface ContentKnowledgeControllerOptions {
  bus?: EventBus;
  store?: KnowledgeLinkStore;
}

/**
 * Controller wiring Content Editor `BlockChanged` events into the Knowledge
 * Network link index, then emitting `BacklinksUpdated`.
 */
export class ContentKnowledgeController {
  private readonly bus: EventBus;
  private readonly store: KnowledgeLinkStore;
  private readonly sub: Subscription;
  private readonly emitBacklinks: (docId: string) => void;

  constructor(options: ContentKnowledgeControllerOptions = {}) {
    this.bus = options.bus ?? new EventBus();
    this.store = options.store ?? new InMemoryKnowledgeLinkStore();
    this.emitBacklinks = (docId: string): void => {
      const event: CoreEvent = { type: 'BacklinksUpdated', doc_id: docId };
      this.bus.publish(event);
    };
    this.sub = this.bus.subscribe('BlockChanged', (event) => {
      if (event.type !== 'BlockChanged') return;
      this.handleBlockChanged(event).catch(() => {
        // swallow — mock controller; real impl would surface errors.
      });
    });
  }

  /** The bus the controller is bound to. */
  get eventBus(): EventBus {
    return this.bus;
  }

  /** The link-index store. */
  get linkStore(): KnowledgeLinkStore {
    return this.store;
  }

  /** Synchronously parse + index links for a block. Exposed for testing. */
  indexBlock(event: Extract<CoreEvent, { type: 'BlockChanged' }>): Link[] {
    const text = contentToString(event.content);
    const parsed: ParsedLinks = {
      wikiLinks: parseWikiLinks(text),
      markdownLinks: parseMarkdownLinks(text),
    };
    const links = toLinks(
      event.doc_id,
      event.block_id,
      parsed,
      new Date().toISOString(),
    );
    // Merge with any existing links from other blocks in the same doc.
    const existing = this.store
      .getLinksForDoc(event.doc_id)
      .filter((l) => l.block_id !== event.block_id);
    const merged = [...existing, ...links];
    this.store.setLinksForDoc(event.doc_id, merged);
    return links;
  }

  private async handleBlockChanged(
    event: Extract<CoreEvent, { type: 'BlockChanged' }>,
  ): Promise<void> {
    this.indexBlock(event);
    this.emitBacklinks(event.doc_id);
  }

  /** Detach from the bus. */
  dispose(): void {
    this.sub.unsubscribe();
  }
}

/** React hook return shape. */
export interface UseContentKnowledgeLink {
  /** Links observed for `docId` so far. */
  links: Link[];
  /** Force a refresh of the links for `docId`. */
  refresh: (docId: string) => void;
}

/**
 * Subscribe to backlinks updates for `docId` and expose the latest link set.
 *
 * The controller is created once and shared across renders; pass an external
 * `controller` when coordinating with other hooks.
 */
export function useContentKnowledgeLink(
  docId: string,
  controller?: ContentKnowledgeController,
): UseContentKnowledgeLink {
  const ref = useRef<ContentKnowledgeController | null>(controller ?? null);
  if (!ref.current) {
    ref.current = new ContentKnowledgeController();
  }
  const ctrl = ref.current;

  const [links, setLinks] = useState<Link[]>(() =>
    ctrl.linkStore.getLinksForDoc(docId),
  );

  useEffect(() => {
    const sub = ctrl.eventBus.subscribe('BacklinksUpdated', (event) => {
      if (event.type === 'BacklinksUpdated' && event.doc_id === docId) {
        setLinks(ctrl.linkStore.getLinksForDoc(docId));
      }
    });
    setLinks(ctrl.linkStore.getLinksForDoc(docId));
    return () => sub.unsubscribe();
  }, [ctrl, docId]);

  const refresh = useMemo(
    () => (id: string) => setLinks(ctrl.linkStore.getLinksForDoc(id)),
    [ctrl],
  );

  return { links, refresh };
}
