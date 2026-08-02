/**
 * SubTask 5.4.3 — AI ↔ Content ↔ Knowledge interaction.
 *
 * The {@link AiInteractionController} consumes `AIGenerationComplete` events
 * (the terminal event of the AI streaming flow) and `SemanticSearch` results
 * to drive graph highlighting in the Knowledge Network. The controller stores
 * the latest AI output and the set of doc ids currently highlighted by a
 * semantic search.
 *
 * The hook {@link useAiInteraction} exposes the latest AI output and the
 * highlighted-doc set to React.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import type { CoreEvent, SearchResult } from '@aurora/shared-types';
import { EventBus, type Subscription } from './eventBus';

/** A semantic-search request descriptor (frontend-only payload). */
export interface SemanticSearchRequest {
  query: string;
  results: SearchResult[];
}

/** Minimal AI output store. */
export interface AiOutputStore {
  setLatest(requestId: string, output: string): void;
  getLatest(): { requestId: string; output: string } | null;
}

/** In-memory {@link AiOutputStore}. */
export class InMemoryAiOutputStore implements AiOutputStore {
  private latest: { requestId: string; output: string } | null = null;

  setLatest(requestId: string, output: string): void {
    this.latest = { requestId, output };
  }

  getLatest(): { requestId: string; output: string } | null {
    return this.latest;
  }
}

/** Minimal highlight store: the set of doc ids currently highlighted. */
export interface HighlightStore {
  setHighlighted(docIds: string[]): void;
  getHighlighted(): string[];
}

/** In-memory {@link HighlightStore}. */
export class InMemoryHighlightStore implements HighlightStore {
  private highlighted: string[] = [];

  setHighlighted(docIds: string[]): void {
    this.highlighted = [...docIds];
  }

  getHighlighted(): string[] {
    return [...this.highlighted];
  }
}

export interface AiInteractionControllerOptions {
  bus?: EventBus;
  outputStore?: AiOutputStore;
  highlightStore?: HighlightStore;
}

/**
 * Controller wiring AI generation + semantic-search results into the
 * Knowledge Network highlight surface.
 */
export class AiInteractionController {
  private readonly bus: EventBus;
  private readonly outputStore: AiOutputStore;
  private readonly highlightStore: HighlightStore;
  private readonly subs: Subscription[] = [];

  constructor(options: AiInteractionControllerOptions = {}) {
    this.bus = options.bus ?? new EventBus();
    this.outputStore = options.outputStore ?? new InMemoryAiOutputStore();
    this.highlightStore = options.highlightStore ?? new InMemoryHighlightStore();

    this.subs.push(
      this.bus.subscribe('AIGenerationComplete', (event) => {
        if (event.type !== 'AIGenerationComplete') return;
        this.handleAiComplete(event);
      }),
    );
  }

  get eventBus(): EventBus {
    return this.bus;
  }

  get outputs(): AiOutputStore {
    return this.outputStore;
  }

  get highlights(): HighlightStore {
    return this.highlightStore;
  }

  /** Record a completed AI generation. */
  recordCompletion(
    event: Extract<CoreEvent, { type: 'AIGenerationComplete' }>,
  ): void {
    this.outputStore.setLatest(event.request_id, event.output);
  }

  /**
   * Apply a semantic-search result set: highlight the matching docs and emit
   * an `IndexRebuildRequest` (link index) so the graph refreshes.
   */
  applySemanticSearch(request: SemanticSearchRequest): void {
    const docIds = request.results.map((r) => r.doc_id);
    this.highlightStore.setHighlighted(docIds);
    const event: CoreEvent = { type: 'IndexRebuildRequest', index_type: 'link' };
    this.bus.publish(event);
  }

  /** Currently-highlighted doc ids (from the last semantic search). */
  highlightedDocs(): string[] {
    return this.highlightStore.getHighlighted();
  }

  private handleAiComplete(
    event: Extract<CoreEvent, { type: 'AIGenerationComplete' }>,
  ): void {
    this.recordCompletion(event);
    // When AI references docs by id in its output, highlight them in the graph.
    const referenced = extractDocReferences(event.output);
    if (referenced.length > 0) {
      this.highlightStore.setHighlighted(referenced);
    }
  }

  dispose(): void {
    for (const sub of this.subs) sub.unsubscribe();
  }
}

/**
 * Extract `doc:<id>` references from an AI output string. The AI backend is
 * expected to emit references in this canonical form so the view layer can
 * cross-link content ↔ knowledge.
 */
export function extractDocReferences(output: string): string[] {
  const re = /doc:([A-Za-z0-9_-]+)/g;
  const out: string[] = [];
  let match: RegExpExecArray | null;
  while ((match = re.exec(output)) !== null) {
    out.push(match[1]);
  }
  return out;
}

/** React hook return shape. */
export interface UseAiInteraction {
  /** Latest AI completion (request id + output). */
  latest: { requestId: string; output: string } | null;
  /** Doc ids currently highlighted by AI/semantic search. */
  highlighted: string[];
}

/**
 * Expose the AI interaction controller state to React.
 */
export function useAiInteraction(
  controller?: AiInteractionController,
): UseAiInteraction {
  const ref = useRef<AiInteractionController | null>(controller ?? null);
  if (!ref.current) {
    ref.current = new AiInteractionController();
  }
  const ctrl = ref.current;

  const [latest, setLatest] = useState<
    { requestId: string; output: string } | null
  >(() => ctrl.outputs.getLatest());
  const [highlighted, setHighlighted] = useState<string[]>(() =>
    ctrl.highlightedDocs(),
  );

  useEffect(() => {
    const onComplete = ctrl.eventBus.subscribe('AIGenerationComplete', () => {
      setLatest(ctrl.outputs.getLatest());
      setHighlighted(ctrl.highlightedDocs());
    });
    const onRebuild = ctrl.eventBus.subscribe('IndexRebuildRequest', () => {
      setHighlighted(ctrl.highlightedDocs());
    });
    return () => {
      onComplete.unsubscribe();
      onRebuild.unsubscribe();
    };
  }, [ctrl]);

  return useMemo(
    () => ({ latest, highlighted }),
    [latest, highlighted],
  );
}
