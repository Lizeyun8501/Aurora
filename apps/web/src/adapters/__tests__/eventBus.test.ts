/**
 * EventBus tests — subscribe/publish/unsubscribe, multiple subscribers,
 * typed event filtering, wildcard, error isolation, listenerCount, clear.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { EventBus } from '../interactions/eventBus';
import type { CoreEvent } from '@aurora/shared-types';

describe('EventBus — subscribe/publish', () => {
  it('delivers a published event to a matching subscriber', () => {
    const bus = new EventBus();
    const handler = vi.fn();
    bus.subscribe('BlockChanged', handler);
    const event: CoreEvent = {
      type: 'BlockChanged',
      doc_id: 'd1',
      block_id: 'b1',
      block_type: 'text',
      content: 'hello',
    };
    bus.publish(event);
    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith(event);
  });

  it('does not deliver events of a different type to a typed subscriber', () => {
    const bus = new EventBus();
    const handler = vi.fn();
    bus.subscribe('BlockChanged', handler);
    bus.publish({ type: 'TaskCreated', task_id: 't1', title: 'task' });
    expect(handler).not.toHaveBeenCalled();
  });

  it('delivers the typed event payload (narrowed)', () => {
    const bus = new EventBus();
    let captured: string | null = null;
    bus.subscribe('TaskCreated', (e) => {
      captured = e.title;
    });
    bus.publish({ type: 'TaskCreated', task_id: 't1', title: 'My Task' });
    expect(captured).toBe('My Task');
  });
});

describe('EventBus — unsubscribe', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('unsubscribes via the returned token', () => {
    const bus = new EventBus();
    const handler = vi.fn();
    const sub = bus.subscribe('BlockChanged', handler);
    expect(sub.active).toBe(true);
    sub.unsubscribe();
    expect(sub.active).toBe(false);
    bus.publish({
      type: 'BlockChanged',
      doc_id: 'd1',
      block_id: 'b1',
      block_type: 'text',
      content: 'x',
    });
    expect(handler).not.toHaveBeenCalled();
  });

  it('unsubscribes via bus.unsubscribe(handler)', () => {
    const bus = new EventBus();
    const handler = vi.fn();
    bus.subscribe('TaskUpdated', handler);
    bus.unsubscribe(handler);
    bus.publish({ type: 'TaskUpdated', task_id: 't1', status: 'done' });
    expect(handler).not.toHaveBeenCalled();
  });

  it('unsubscribe is idempotent', () => {
    const bus = new EventBus();
    const handler = vi.fn();
    const sub = bus.subscribe('TaskUpdated', handler);
    sub.unsubscribe();
    sub.unsubscribe(); // no-op
    expect(bus.listenerCount).toBe(0);
  });
});

describe('EventBus — multiple subscribers', () => {
  it('delivers to all matching subscribers', () => {
    const bus = new EventBus();
    const h1 = vi.fn();
    const h2 = vi.fn();
    bus.subscribe('BlockChanged', h1);
    bus.subscribe('BlockChanged', h2);
    bus.publish({
      type: 'BlockChanged',
      doc_id: 'd1',
      block_id: 'b1',
      block_type: 'text',
      content: 'x',
    });
    expect(h1).toHaveBeenCalledTimes(1);
    expect(h2).toHaveBeenCalledTimes(1);
  });

  it('wildcard subscriber receives every event type', () => {
    const bus = new EventBus();
    const all = vi.fn();
    bus.subscribe('*', all);
    bus.publish({ type: 'TaskCreated', task_id: 't1', title: 'a' });
    bus.publish({ type: 'PluginLoaded', plugin_id: 'p1' });
    expect(all).toHaveBeenCalledTimes(2);
  });
});

describe('EventBus — error isolation', () => {
  it('a throwing handler does not break sibling handlers', () => {
    const onError = vi.fn();
    const bus = new EventBus({ onError });
    const good = vi.fn();
    bus.subscribe('BlockChanged', () => {
      throw new Error('boom');
    });
    bus.subscribe('BlockChanged', good);
    bus.publish({
      type: 'BlockChanged',
      doc_id: 'd1',
      block_id: 'b1',
      block_type: 'text',
      content: 'x',
    });
    expect(good).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledTimes(1);
  });
});

describe('EventBus — listenerCount / clear', () => {
  it('reports listener count and clears all', () => {
    const bus = new EventBus();
    bus.subscribe('BlockChanged', () => undefined);
    bus.subscribe('TaskCreated', () => undefined);
    expect(bus.listenerCount).toBe(2);
    bus.clear();
    expect(bus.listenerCount).toBe(0);
  });
});
