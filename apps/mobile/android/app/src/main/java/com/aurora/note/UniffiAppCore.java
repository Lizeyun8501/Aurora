package com.aurora.note;

/**
 * JNI bindings for aurora-mobile-ffi native library.
 */
public class UniffiAppCore {

    static {
        try {
            System.loadLibrary("aurora_mobile_ffi");
        } catch (UnsatisfiedLinkError e) {
            throw new RuntimeException("Failed to load aurora_mobile_ffi: " + e.getMessage());
        }
    }

    private long handle;

    public static class NoteSummary {
        public final String id;
        public final String title;
        public final String updatedAt;

        public NoteSummary(String id, String title, String updatedAt) {
            this.id = id;
            this.title = title;
            this.updatedAt = updatedAt;
        }
    }

    public static class SearchResult {
        public final String noteId;
        public final String title;
        public final String snippet;
        public final double score;

        public SearchResult(String noteId, String title, String snippet, double score) {
            this.noteId = noteId;
            this.title = title;
            this.snippet = snippet;
            this.score = score;
        }
    }

    public UniffiAppCore(String dataDir) throws Exception {
        handle = nativeNew(dataDir);
        if (handle == 0) {
            throw new Exception("Failed to initialize core");
        }
    }

    /** native 句柄（供 SyncEngine JNI 桥使用）。 */
    long handle() {
        return handle;
    }

    public String createNote(String title) throws Exception {
        String id = nativeCreateNote(handle, title);
        if (id == null) {
            throw new Exception("Failed to create note");
        }
        return id;
    }

    public java.util.List<NoteSummary> listNotes() {
        int count = nativeListNotesCount(handle);
        java.util.List<NoteSummary> notes = new java.util.ArrayList<>(count);
        for (int i = 0; i < count; i++) {
            String[] parts = nativeGetNote(handle, i);
            if (parts != null && parts.length >= 3) {
                notes.add(new NoteSummary(parts[0], parts[1], parts[2]));
            }
        }
        return notes;
    }

    public java.util.List<SearchResult> searchNotes(String query) {
        int count = nativeSearchCount(handle, query);
        java.util.List<SearchResult> results = new java.util.ArrayList<>(count);
        for (int i = 0; i < count; i++) {
            Object[] parts = nativeGetSearchResult(handle, i, query);
            if (parts != null && parts.length >= 4) {
                String noteId = parts[0] != null ? parts[0].toString() : "";
                String title = parts[1] != null ? parts[1].toString() : "";
                String snippet = parts[2] != null ? parts[2].toString() : "";
                double score = 0;
                if (parts[3] instanceof Number) {
                    score = ((Number) parts[3]).doubleValue();
                }
                results.add(new SearchResult(noteId, title, snippet, score));
            }
        }
        return results;
    }

    public void deleteNote(String noteId) throws Exception {
        int rc = nativeDeleteNote(handle, noteId);
        if (rc != 0) {
            throw new Exception("Failed to delete note: " + noteId);
        }
    }

    public String getNoteContent(String noteId) throws Exception {
        String content = nativeGetNoteContent(handle, noteId);
        if (content == null) {
            throw new Exception("Failed to get note content: " + noteId);
        }
        return content;
    }

    public void saveNoteContent(String noteId, String content) throws Exception {
        int rc = nativeSaveNoteContent(handle, noteId, content);
        if (rc != 0) {
            throw new Exception("Failed to save note: " + noteId);
        }
    }

    /** 获取笔记 Loro 快照（base64）— ProseMirror 编辑器初始化（DEV-009）。 */
    public String getNoteSnapshot(String noteId) {
        return nativeGetNoteSnapshot(handle, noteId);
    }

    /** 保存 JS 侧 Loro 快照（base64，CRDT 合并语义）。失败返回 false。 */
    public boolean saveNoteSnapshot(String noteId, String snapshotBase64) {
        return nativeSaveNoteSnapshot(handle, noteId, snapshotBase64) != 0;
    }

    public boolean isFallback() {
        if (handle == 0) return true;
        return nativeIsFallback(handle) != 0;
    }

    public void destroy() {
        if (handle != 0) {
            nativeDestroy(handle);
            handle = 0;
        }
    }

    @Override
    protected void finalize() throws Throwable {
        try { destroy(); } finally { super.finalize(); }
    }

    private static native long nativeNew(String dataDir);
    private static native String nativeCreateNote(long handle, String title);
    private static native int nativeListNotesCount(long handle);
    private static native String[] nativeGetNote(long handle, int index);
    private static native int nativeSearchCount(long handle, String query);
    private static native Object[] nativeGetSearchResult(long handle, int index, String query);
    private static native int nativeDeleteNote(long handle, String noteId);
    private static native int nativeIsFallback(long handle);
    private static native int nativeSaveNoteContent(long handle, String noteId, String content);
    private static native String nativeGetNoteContent(long handle, String noteId);
    private static native String nativeGetNoteSnapshot(long handle, String noteId);
    private static native int nativeSaveNoteSnapshot(long handle, String noteId, String snapshotBase64);
    private static native void nativeDestroy(long handle);
}
