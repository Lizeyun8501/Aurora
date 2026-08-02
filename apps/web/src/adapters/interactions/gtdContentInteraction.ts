/**
 * SubTask 5.4.2 — GTD ↔ Content Editor interaction.
 *
 * The {@link GtdContentController} listens for `TaskCreated` events and embeds
 * a `task_block` into the active document (mock mutation), and for
 * `TaskUpdated` events to refresh the embedded task's status + the document's
 * task count. The hook {@link useGtdContentLink} exposes the per-document task
 * count and the list of embedded task blocks to React.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import type { CoreEvent, JsonValue, TaskStatus } from '@aurora/shared-types';
import { EventBus, type Subscription } from './eventBus';

/** A minimal content-store surface for embedding task blocks. */
export interface ContentBlockStore {
  /** Insert (or upsert) a block record under `docId`. */
  upsertBlock(docId: string, block: TaskBlockRecord): void;
  /** Remove a block by id. */
  removeBlock(docId: string, blockId: string): void;
  /** Read all blocks for `docId`. */
  getBlocks(docId: string): TaskBlockRecord[];
}

/** An embedded task block record (mock of the editor's Block node). */
export interface TaskBlockRecord {
  block_id: string;
  task_id: string;
  title: string;
  status: TaskStatus;
  created_at: string;
}

/** In-memory {@link ContentBlockStore}. */
export class InMemoryContentBlockStore implements ContentBlockStore {
  private readonly byDoc = new Map<string, TaskBlockRecord[]>();

  upsertBlock(docId: string, block: TaskBlockRecord): void {
    const list = this.byDoc.get(docId) ?? [];
    const idx = list.findIndex((b) => b.block_id === block.block_id);
    if (idx >= 0) list[idx] = block;
    else list.push(block);
    this.byDoc.set(docId, list);
  }

  removeBlock(docId: string, blockId: string): void {
    const list = this.byDoc.get(docId) ?? [];
    this.byDoc.set(
      docId,
      list.filter((b) => b.block_id !== blockId),
    );
  }

  getBlocks(docId: string): TaskBlockRecord[] {
    return this.byDoc.get(docId) ?? [];
  }
}

/** Default document id used when a TaskCreated event carries no doc reference. */
export const DEFAULT_DOC_ID = 'inbox';

/** The block_type string emitted for embedded task blocks. */
export const TASK_BLOCK_TYPE = 'task_block';

export interface GtdContentControllerOptions {
  bus?: EventBus;
  store?: ContentBlockStore;
  /** Active document id (defaults to {@link DEFAULT_DOC_ID}). */
  activeDocId?: string;
}

/**
 * Controller wiring GTD `TaskCreated` / `TaskUpdated` events into the Content
 * Editor by embedding/refreshing `task_block` blocks.
 */
export class GtdContentController {
  private readonly bus: EventBus;
  private readonly store: ContentBlockStore;
  private activeDocId: string;
  private readonly subs: Subscription[] = [];

  constructor(options: GtdContentControllerOptions = {}) {
    this.bus = options.bus ?? new EventBus();
    this.store = options.store ?? new InMemoryContentBlockStore();
    this.activeDocId = options.activeDocId ?? DEFAULT_DOC_ID;

    this.subs.push(
      this.bus.subscribe('TaskCreated', (event) => {
        if (event.type !== 'TaskCreated') return;
        this.handleTaskCreated(event).catch(() => {
          // swallow — mock controller.
        });
      }),
    );

    this.subs.push(
      this.bus.subscribe('TaskUpdated', (event) => {
        if (event.type !== 'TaskUpdated') return;
        this.handleTaskUpdated(event);
      }),
    );
  }

  get eventBus(): EventBus {
    return this.bus;
  }

  get blockStore(): ContentBlockStore {
    return this.store;
  }

  get docId(): string {
    return this.activeDocId;
  }

  setActiveDoc(docId: string): void {
    this.activeDocId = docId;
  }

  /** Embed a task block for a TaskCreated event. Returns the block record. */
  embedTask(
    event: Extract<CoreEvent, { type: 'TaskCreated' }>,
  ): TaskBlockRecord {
    const block: TaskBlockRecord = {
      block_id: `task-block:${event.task_id}`,
      task_id: event.task_id,
      title: event.title,
      status: 'inbox',
      created_at: new Date().toISOString(),
    };
    this.store.upsertBlock(this.activeDocId, block);
    return block;
  }

  /** Update the status of an embedded task block. Returns true if found. */
  updateTaskStatus(
    event: Extract<CoreEvent, { type: 'TaskUpdated' }>,
  ): boolean {
    const blocks = this.store.getBlocks(this.activeDocId);
    const target = blocks.find((b) => b.task_id === event.task_id);
    if (!target) return false;
    this.store.upsertBlock(this.activeDocId, {
      ...target,
      status: event.status as TaskStatus,
    });
    return true;
  }

  /** Count of embedded task blocks for the active doc. */
  taskCount(): number {
    return this.store.getBlocks(this.activeDocId).length;
  }

  private async handleTaskCreated(
    event: Extract<CoreEvent, { type: 'TaskCreated' }>,
  ): Promise<void> {
    this.embedTask(event);
    // Re-emit a BlockChanged so the content/knowledge pipeline can react.
    const blockEvent: CoreEvent = {
      type: 'BlockChanged',
      doc_id: this.activeDocId,
      block_id: `task-block:${event.task_id}`,
      block_type: TASK_BLOCK_TYPE,
      content: { task_id: event.task_id, title: event.title } as JsonValue,
    };
    this.bus.publish(blockEvent);
  }

  private handleTaskUpdated(
    event: Extract<CoreEvent, { type: 'TaskUpdated' }>,
  ): void {
    this.updateTaskStatus(event);
  }

  dispose(): void {
    for (const sub of this.subs) sub.unsubscribe();
  }
}

/** React hook return shape. */
export interface UseGtdContentLink {
  /** Embedded task blocks for the active doc. */
  blocks: TaskBlockRecord[];
  /** Number of embedded task blocks. */
  taskCount: number;
}

/**
 * Expose the GTD↔Content controller state for `docId` to React.
 */
export function useGtdContentLink(
  docId: string = DEFAULT_DOC_ID,
  controller?: GtdContentController,
): UseGtdContentLink {
  const ref = useRef<GtdContentController | null>(controller ?? null);
  if (!ref.current) {
    ref.current = new GtdContentController({ activeDocId: docId });
  }
  const ctrl = ref.current;
  ctrl.setActiveDoc(docId);

  const [blocks, setBlocks] = useState<TaskBlockRecord[]>(() =>
    ctrl.blockStore.getBlocks(docId),
  );

  useEffect(() => {
    const refresh = (): void => setBlocks(ctrl.blockStore.getBlocks(docId));
    refresh();
    const created = ctrl.eventBus.subscribe('TaskCreated', () => refresh());
    const updated = ctrl.eventBus.subscribe('TaskUpdated', () => refresh());
    return () => {
      created.unsubscribe();
      updated.unsubscribe();
    };
  }, [ctrl, docId]);

  const taskCount = useMemo(() => blocks.length, [blocks]);

  return { blocks, taskCount };
}
