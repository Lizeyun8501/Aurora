import type {
  Block,
  BlockType,
  Device,
  Document,
  JsonValue,
  Task,
} from '@aurora/shared-types';

let counter = 0;
const nid = (prefix: string): string => `${prefix}-${++counter}`;

export function block(
  blockType: BlockType,
  content: JsonValue = {},
  overrides: Partial<Block> = {},
): Block {
  return {
    id: nid('b'),
    block_type: blockType,
    content,
    properties: {},
    children: [],
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z',
    ...overrides,
  };
}

export function doc(blocks: Block[] = [], overrides: Partial<Document> = {}): Document {
  return {
    id: 'doc-1',
    title: 'Test Doc',
    blocks,
    properties: {},
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z',
    version: 1,
    ...overrides,
  };
}

export function task(overrides: Partial<Task> = {}): Task {
  return {
    id: nid('t'),
    title: 'Task',
    description: null,
    status: 'inbox',
    priority: 'medium',
    project_id: null,
    parent_id: null,
    due_date: null,
    scheduled_date: null,
    completed_at: null,
    tags: [],
    context: null,
    estimated_minutes: null,
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z',
    energy_level: null,
    ...overrides,
  };
}

export function device(overrides: Partial<Device> = {}): Device {
  return {
    id: nid('dev'),
    name: 'Device',
    platform: 'linux',
    status: 'offline',
    last_seen: null,
    ...overrides,
  };
}
