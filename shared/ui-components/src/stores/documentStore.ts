import { create } from 'zustand';
import type { Block, Document } from '@aurora/shared-types';

export interface DocumentState {
  currentDocument: Document | null;
  blocks: Block[];
  dirty: boolean;
  openDocument: (doc: Document) => void;
  updateBlock: (blockId: string, updates: Partial<Block>) => void;
  addBlock: (block: Block) => void;
  removeBlock: (blockId: string) => void;
  save: () => Promise<void>;
}

const now = (): string => new Date().toISOString();

export const useDocumentStore = create<DocumentState>()((set, get) => ({
  currentDocument: null,
  blocks: [],
  dirty: false,
  openDocument: (doc) =>
    set({ currentDocument: doc, blocks: doc.blocks, dirty: false }),
  updateBlock: (blockId, updates) =>
    set((state) => ({
      blocks: state.blocks.map((b) =>
        b.id === blockId ? { ...b, ...updates, updated_at: now() } : b,
      ),
      dirty: true,
    })),
  addBlock: (block) =>
    set((state) => ({ blocks: [...state.blocks, block], dirty: true })),
  removeBlock: (blockId) =>
    set((state) => ({
      blocks: state.blocks.filter((b) => b.id !== blockId),
      dirty: true,
    })),
  save: async () => {
    const { currentDocument, blocks } = get();
    if (currentDocument) {
      set({
        currentDocument: {
          ...currentDocument,
          blocks,
          updated_at: now(),
          version: currentDocument.version + 1,
        },
        dirty: false,
      });
    } else {
      set({ dirty: false });
    }
  },
}));
