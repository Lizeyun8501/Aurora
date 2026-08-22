/**
 * Android WebView 平台适配器 — V19 §36.3 正式方案。
 *
 * 数据链路: React → window.AndroidBridge (@JavascriptInterface)
 *         → Java UniffiAppCore → JNI → Rust aurora-mobile-ffi → Core
 *
 * 当运行在浏览器（开发预览）时自动降级为 localStorage mock。
 */

export interface NoteSummary {
  id: string;
  title: string;
  updatedAt: string;
}

export interface SearchResult {
  noteId: string;
  title: string;
  snippet: string;
  score: number;
}

interface AndroidBridge {
  init(dataDir: string): string;
  listNotes(): string;
  createNote(title: string): string;
  getNoteContent(noteId: string): string;
  saveNoteContent(noteId: string, content: string): number;
  deleteNote(noteId: string): number;
  searchNotes(query: string): string;
  isFallback(): boolean;
}

declare global {
  interface Window {
    AndroidBridge?: AndroidBridge;
  }
}

function bridge(): AndroidBridge | undefined {
  return typeof window !== 'undefined' ? window.AndroidBridge : undefined;
}

/** 是否运行在 Android WebView 内。 */
export function isAndroid(): boolean {
  return bridge() !== undefined;
}

// ---------------------------------------------------------------------------
// localStorage mock — 浏览器开发预览用
// ---------------------------------------------------------------------------

interface StoredNote {
  id: string;
  title: string;
  content: string;
  updatedAt: string;
}

function mockNotes(): StoredNote[] {
  try {
    return JSON.parse(localStorage.getItem('aurora.notes') ?? '[]') as StoredNote[];
  } catch {
    return [];
  }
}

function mockSave(notes: StoredNote[]): void {
  localStorage.setItem('aurora.notes', JSON.stringify(notes));
}

// ---------------------------------------------------------------------------
// 平台 API — 与 Rust CoreAPI 对齐 (V19 §36.3)
// ---------------------------------------------------------------------------

export const platform = {
  init(dataDir: string): boolean {
    const b = bridge();
    if (b) return b.init(dataDir) === 'ok';
    return true; // mock 模式永远成功
  },

  isFallback(): boolean {
    return bridge()?.isFallback() ?? false;
  },

  listNotes(): NoteSummary[] {
    const b = bridge();
    if (b) {
      try {
        return JSON.parse(b.listNotes()) as NoteSummary[];
      } catch {
        return [];
      }
    }
    return mockNotes().map(({ id, title, updatedAt }) => ({ id, title, updatedAt }));
  },

  createNote(title: string): string | null {
    const b = bridge();
    if (b) {
      const id = b.createNote(title);
      return id || null;
    }
    const notes = mockNotes();
    const note: StoredNote = {
      id: `note-${Date.now()}`,
      title,
      content: '',
      updatedAt: new Date().toISOString(),
    };
    notes.unshift(note);
    mockSave(notes);
    return note.id;
  },

  getNoteContent(noteId: string): string {
    const b = bridge();
    if (b) return b.getNoteContent(noteId) ?? '';
    return mockNotes().find((n) => n.id === noteId)?.content ?? '';
  },

  saveNoteContent(noteId: string, content: string): boolean {
    const b = bridge();
    if (b) return b.saveNoteContent(noteId, content) === 0;
    const notes = mockNotes();
    const n = notes.find((x) => x.id === noteId);
    if (n) {
      n.content = content;
      n.updatedAt = new Date().toISOString();
      mockSave(notes);
      return true;
    }
    return false;
  },

  deleteNote(noteId: string): boolean {
    const b = bridge();
    if (b) return b.deleteNote(noteId) === 0;
    mockSave(mockNotes().filter((n) => n.id !== noteId));
    return true;
  },

  searchNotes(query: string): SearchResult[] {
    const b = bridge();
    if (b) {
      try {
        return JSON.parse(b.searchNotes(query)) as SearchResult[];
      } catch {
        return [];
      }
    }
    const q = query.toLowerCase();
    return mockNotes()
      .filter((n) => n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q))
      .map((n) => ({
        noteId: n.id,
        title: n.title,
        snippet: n.content.slice(0, 80),
        score: 1,
      }));
  },
};
