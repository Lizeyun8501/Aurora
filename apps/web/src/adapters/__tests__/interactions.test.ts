/**
 * Cross-module interaction controller tests.
 *
 * Covers the four interaction flows:
 *  - Content ↔ Knowledge (BlockChanged → BacklinksUpdated + link index)
 *  - GTD ↔ Content (TaskCreated → embed task block; TaskUpdated → status)
 *  - AI ↔ Content ↔ Knowledge (AIGenerationComplete → output + highlights)
 *  - Sync orchestrator (enqueue on any event, dispatch to targets)
 */

import { describe, expect, it } from 'vitest';
import type { CoreEvent } from '@aurora/shared-types';
import { EventBus } from '../interactions/eventBus';
import {
  ContentKnowledgeController,
  parseWikiLinks,
  parseMarkdownLinks,
} from '../interactions/contentKnowledgeInteraction';
import { GtdContentController } from '../interactions/gtdContentInteraction';
import {
  AiInteractionController,
  extractDocReferences,
} from '../interactions/aiContentKnowledgeInteraction';
import {
  SyncOrchestrator,
  MockSyncDispatcher,
} from '../interactions/syncCrossCutting';

// ---------------------------------------------------------------------------
// Content ↔ Knowledge
// ---------------------------------------------------------------------------

describe('ContentKnowledgeController — link parsing', () => {
  it('parses [[wiki]] and [[wiki|display]] links', () => {
    const links = parseWikiLinks('see [[alpha]] and [[beta|the beta]]');
    expect(links).toEqual([
      { target: 'alpha', display_text: null },
      { target: 'beta', display_text: 'the beta' },
    ]);
  });

  it('parses [text](target) markdown links but skips images', () => {
    const links = parseMarkdownLinks(
      'see [a](doc1.md) and ![img](pic.png) and [b](doc2.md "title")',
    );
    expect(links.map((l) => l.target)).toEqual(['doc1.md', 'doc2.md']);
    expect(links.map((l) => l.text)).toEqual(['a', 'b']);
  });
});

describe('ContentKnowledgeController — event flow', () => {
  it('indexes links on BlockChanged and emits BacklinksUpdated', () => {
    const bus = new EventBus();
    const ctrl = new ContentKnowledgeController({ bus });

    const received: CoreEvent[] = [];
    bus.subscribe('BacklinksUpdated', (e) => received.push(e));

    bus.publish({
      type: 'BlockChanged',
      doc_id: 'd1',
      block_id: 'b1',
      block_type: 'text',
      content: 'links to [[target-a]] and [t](target-b.md)',
    });

    expect(received).toHaveLength(1);
    expect(received[0]).toEqual({ type: 'BacklinksUpdated', doc_id: 'd1' });

    const links = ctrl.linkStore.getLinksForDoc('d1');
    expect(links).toHaveLength(2);
    expect(links.map((l) => l.target_doc_id).sort()).toEqual([
      'target-a',
      'target-b.md',
    ]);

    ctrl.dispose();
  });

  it('merges links from multiple blocks within a doc', () => {
    const bus = new EventBus();
    const ctrl = new ContentKnowledgeController({ bus });
    bus.publish({
      type: 'BlockChanged',
      doc_id: 'd1',
      block_id: 'b1',
      block_type: 'text',
      content: '[[one]]',
    });
    bus.publish({
      type: 'BlockChanged',
      doc_id: 'd1',
      block_id: 'b2',
      block_type: 'text',
      content: '[[two]]',
    });
    const links = ctrl.linkStore.getLinksForDoc('d1');
    expect(links.map((l) => l.block_id).sort()).toEqual(['b1', 'b2']);
    ctrl.dispose();
  });
});

// ---------------------------------------------------------------------------
// GTD ↔ Content
// ---------------------------------------------------------------------------

describe('GtdContentController — event flow', () => {
  it('embeds a task block on TaskCreated', () => {
    const bus = new EventBus();
    const ctrl = new GtdContentController({ bus, activeDocId: 'doc-x' });

    bus.publish({ type: 'TaskCreated', task_id: 't1', title: 'Write tests' });

    const blocks = ctrl.blockStore.getBlocks('doc-x');
    expect(blocks).toHaveLength(1);
    expect(blocks[0].task_id).toBe('t1');
    expect(blocks[0].title).toBe('Write tests');
    expect(ctrl.taskCount()).toBe(1);
    ctrl.dispose();
  });

  it('updates the embedded task status on TaskUpdated', () => {
    const bus = new EventBus();
    const ctrl = new GtdContentController({ bus, activeDocId: 'doc-x' });

    bus.publish({ type: 'TaskCreated', task_id: 't1', title: 'Write tests' });
    bus.publish({ type: 'TaskUpdated', task_id: 't1', status: 'done' });

    const blocks = ctrl.blockStore.getBlocks('doc-x');
    expect(blocks[0].status).toBe('done');
    ctrl.dispose();
  });

  it('re-emits a BlockChanged event for the embedded task block', () => {
    const bus = new EventBus();
    const ctrl = new GtdContentController({ bus, activeDocId: 'doc-x' });

    const seen: CoreEvent[] = [];
    bus.subscribe('BlockChanged', (e) => seen.push(e));

    bus.publish({ type: 'TaskCreated', task_id: 't9', title: 'X' });
    expect(seen).toHaveLength(1);
    expect(seen[0].type).toBe('BlockChanged');
    ctrl.dispose();
  });
});

// ---------------------------------------------------------------------------
// AI ↔ Content ↔ Knowledge
// ---------------------------------------------------------------------------

describe('AiInteractionController — event flow', () => {
  it('records the latest AI completion and emits nothing extraneous', () => {
    const bus = new EventBus();
    const ctrl = new AiInteractionController({ bus });
    bus.publish({
      type: 'AIGenerationComplete',
      request_id: 'r1',
      output: 'Here is a summary. doc:alpha doc:beta',
    });
    const latest = ctrl.outputs.getLatest();
    expect(latest?.requestId).toBe('r1');
    expect(latest?.output).toContain('summary');
    // doc:alpha and doc:beta should be highlighted.
    expect(ctrl.highlightedDocs().sort()).toEqual(['alpha', 'beta']);
    ctrl.dispose();
  });

  it('extracts doc references from AI output', () => {
    expect(extractDocReferences('see doc:one and doc:two_x and doc:three-3'))
      .toEqual(['one', 'two_x', 'three-3']);
  });

  it('applies semantic search results and emits IndexRebuildRequest', () => {
    const bus = new EventBus();
    const ctrl = new AiInteractionController({ bus });

    const seen: CoreEvent[] = [];
    bus.subscribe('IndexRebuildRequest', (e) => seen.push(e));

    ctrl.applySemanticSearch({
      query: 'q',
      results: [
        { doc_id: 'd1', title: 'A', score: 0.9, snippet: null, matched_by: 'vector' },
        { doc_id: 'd2', title: 'B', score: 0.8, snippet: null, matched_by: 'vector' },
      ],
    });
    expect(ctrl.highlightedDocs().sort()).toEqual(['d1', 'd2']);
    expect(seen).toHaveLength(1);
    expect(seen[0]).toEqual({ type: 'IndexRebuildRequest', index_type: 'link' });
    ctrl.dispose();
  });
});

// ---------------------------------------------------------------------------
// Sync orchestrator
// ---------------------------------------------------------------------------

describe('SyncOrchestrator — enqueue + dispatch', () => {
  it('enqueues every event published to the bus', () => {
    const bus = new EventBus();
    const orch = new SyncOrchestrator({ bus });
    bus.publish({ type: 'TaskCreated', task_id: 't1', title: 'a' });
    bus.publish({ type: 'PluginLoaded', plugin_id: 'p1' });
    expect(orch.pendingCount()).toBe(2);
    orch.dispose();
  });

  it('dispatches the queue to every target and emits SyncProgress', async () => {
    const bus = new EventBus();
    const cloud = new MockSyncDispatcher('cloud');
    const p2p = new MockSyncDispatcher('p2p');
    const orch = new SyncOrchestrator({ bus, dispatchers: [cloud, p2p] });

    const progress: CoreEvent[] = [];
    bus.subscribe('SyncProgress', (e) => progress.push(e));

    bus.publish({ type: 'TaskCreated', task_id: 't1', title: 'a' });
    bus.publish({ type: 'TaskCreated', task_id: 't2', title: 'b' });

    await orch.dispatch();

    expect(cloud.calls).toHaveLength(1);
    expect(cloud.calls[0]).toHaveLength(2);
    expect(p2p.calls).toHaveLength(1);
    expect(progress.filter((e) => e.type === 'SyncProgress')).toHaveLength(2);
    expect(orch.pendingCount()).toBe(0);
    orch.dispose();
  });

  it('auto-dispatches when autoDispatch is enabled', async () => {
    const bus = new EventBus();
    const cloud = new MockSyncDispatcher('cloud');
    const orch = new SyncOrchestrator({
      bus,
      dispatchers: [cloud],
      autoDispatch: true,
    });
    bus.publish({ type: 'PluginLoaded', plugin_id: 'p1' });
    // autoDispatch fires async; wait a microtask cycle.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(cloud.calls).toHaveLength(1);
    expect(orch.pendingCount()).toBe(0);
    orch.dispose();
  });
});
