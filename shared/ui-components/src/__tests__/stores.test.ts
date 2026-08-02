import { describe, it, expect, beforeEach } from 'vitest';
import {
  useDocumentStore,
  useTaskStore,
  useUIStore,
  useSyncStore,
} from '../stores';
import { block, device, task } from './fixtures';

describe('documentStore', () => {
  beforeEach(() => {
    useDocumentStore.setState({ currentDocument: null, blocks: [], dirty: false });
  });

  it('adds a block and marks the store dirty', () => {
    const b = block('text', { text: 'hi' });
    useDocumentStore.getState().addBlock(b);
    expect(useDocumentStore.getState().blocks).toHaveLength(1);
    expect(useDocumentStore.getState().dirty).toBe(true);
  });

  it('opens a document and resets dirty', () => {
    const b = block('text', { text: 'hi' });
    useDocumentStore.getState().openDocument({
      id: 'd-9',
      title: 'D',
      blocks: [b],
      properties: {},
      created_at: '',
      updated_at: '',
      version: 1,
    });
    expect(useDocumentStore.getState().currentDocument?.id).toBe('d-9');
    expect(useDocumentStore.getState().blocks).toHaveLength(1);
    expect(useDocumentStore.getState().dirty).toBe(false);
  });

  it('updates a block in place', () => {
    const b = block('text', { text: 'hi' });
    useDocumentStore.getState().addBlock(b);
    useDocumentStore.getState().updateBlock(b.id, { content: { text: 'edited' } });
    expect(useDocumentStore.getState().blocks[0].content).toEqual({ text: 'edited' });
  });

  it('removes a block', () => {
    const b = block('text', { text: 'hi' });
    useDocumentStore.getState().addBlock(b);
    useDocumentStore.getState().removeBlock(b.id);
    expect(useDocumentStore.getState().blocks).toHaveLength(0);
  });

  it('save clears the dirty flag and bumps version', async () => {
    const b = block('text', { text: 'hi' });
    useDocumentStore.getState().openDocument({
      id: 'd-9',
      title: 'D',
      blocks: [b],
      properties: {},
      created_at: '',
      updated_at: '',
      version: 3,
    });
    useDocumentStore.getState().addBlock(block('text', { text: 'more' }));
    expect(useDocumentStore.getState().dirty).toBe(true);
    await useDocumentStore.getState().save();
    expect(useDocumentStore.getState().dirty).toBe(false);
    expect(useDocumentStore.getState().currentDocument?.version).toBe(4);
  });
});

describe('taskStore', () => {
  beforeEach(() => {
    useTaskStore.setState({ tasks: [], projects: [], inboxCount: 0 });
  });

  it('adds a task and counts the inbox', () => {
    useTaskStore.getState().addTask(task({ status: 'inbox' }));
    expect(useTaskStore.getState().inboxCount).toBe(1);
  });

  it('updates a task status and recomputes inboxCount', () => {
    const t = task({ status: 'inbox' });
    useTaskStore.getState().addTask(t);
    useTaskStore.getState().updateTaskStatus(t.id, 'done');
    expect(useTaskStore.getState().tasks[0].status).toBe('done');
    expect(useTaskStore.getState().inboxCount).toBe(0);
  });

  it('clarifyInbox moves a task out of the inbox', () => {
    const t = task({ status: 'inbox' });
    useTaskStore.getState().addTask(t);
    useTaskStore.getState().clarifyInbox(t.id, 'organized');
    expect(useTaskStore.getState().tasks[0].status).toBe('organized');
    expect(useTaskStore.getState().inboxCount).toBe(0);
  });
});

describe('uiStore', () => {
  beforeEach(() => {
    useUIStore.setState({
      sidebarOpen: true,
      activeView: 'today',
      theme: 'light',
      commandPaletteOpen: false,
    });
  });

  it('toggles the sidebar', () => {
    useUIStore.getState().toggleSidebar();
    expect(useUIStore.getState().sidebarOpen).toBe(false);
    useUIStore.getState().toggleSidebar();
    expect(useUIStore.getState().sidebarOpen).toBe(true);
  });

  it('sets the active view and theme', () => {
    useUIStore.getState().setActiveView('graph');
    useUIStore.getState().setTheme('dark');
    expect(useUIStore.getState().activeView).toBe('graph');
    expect(useUIStore.getState().theme).toBe('dark');
  });
});

describe('syncStore', () => {
  beforeEach(() => {
    useSyncStore.setState({ status: 'idle', devices: [], lastSync: null });
  });

  it('startSync sets status to syncing and records lastSync', () => {
    useSyncStore.getState().startSync();
    expect(useSyncStore.getState().status).toBe('syncing');
    expect(useSyncStore.getState().lastSync).not.toBeNull();
  });

  it('adds and revokes devices', () => {
    const d = device({ id: 'dev-x' });
    useSyncStore.getState().addDevice(d);
    expect(useSyncStore.getState().devices).toHaveLength(1);
    useSyncStore.getState().revokeDevice('dev-x');
    expect(useSyncStore.getState().devices).toHaveLength(0);
  });
});
