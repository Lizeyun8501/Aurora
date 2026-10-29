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
        /** DK-07 S4: 加密级别（"none" | "aes256gcm"）— UI 锁标数据面 */
        public final String encryption;

        public NoteSummary(String id, String title, String updatedAt, String encryption) {
            this.id = id;
            this.title = title;
            this.updatedAt = updatedAt;
            this.encryption = encryption;
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

    /** 查询「仅 Wi-Fi 同步」（DK-08 §7.3；fallback 恒 false）。 */
    public boolean getWifiOnly() throws Exception {
        return nativeGetWifiOnly(handle);
    }

    /** 设置「仅 Wi-Fi 同步」（KV 持久化 + 门即时切换）。 */
    public void setWifiOnly(boolean on) throws Exception {
        nativeSetWifiOnly(handle, on);
    }

    // ------------------------------------------------------------------
    // DK-49：移动端同步数据面（对齐桌面 sync_commands 七函数 —
    // JSON 字符串返回与桌面 DTO 同构；错误抛 Exception 由 bridge 层 catch）。
    // ------------------------------------------------------------------

    /** 函数 1：同步状态聚合 JSON（gate/wifi_only/queue_len/pending_*）。 */
    public String syncStatus() {
        return nativeSyncStatus(handle);
    }

    /** 函数 2：离线队列明细 JSON 数组（QueueItemDto 同构）。 */
    public String queueList() {
        return nativeQueueList(handle);
    }

    /** 函数 3：手动补发 JSON（DrainReportDto 同构）；前置不满足抛异常。 */
    public String queueDrain() throws Exception {
        String r = nativeQueueDrain(handle);
        if (r == null) {
            throw new Exception("queue drain failed (native threw)");
        }
        return r;
    }

    /** 函数 4：待处理冲突合并视图 JSON 数组（ConflictDto 同构）。 */
    public String conflictList() {
        return nativeConflictList(handle);
    }

    /** 函数 5：真冲突副本闭环（mark_resolved — 副本留存人工确认）。 */
    public void conflictResolve(String artifactId) throws Exception {
        nativeConflictResolve(handle, artifactId);
    }

    /** 函数 6：语义冲突解决（LocalWins/RemoteWins/LastWriteWins/Branch）。 */
    public void conflictResolveSemantic(String conflictId, String resolution) throws Exception {
        nativeConflictResolveSemantic(handle, conflictId, resolution);
    }

    /** DK-49 装配辅助：挂接 P2P 引擎（startSyncEngine 注入，drain 通道）。 */
    public void attachSyncEngine(SyncEngine engine) {
        nativeAttachSyncEngine(handle, engine.engineHandle());
    }

    /** DK-49 装配辅助：摘除 P2P 引擎（closeSyncEngine 注销）。 */
    public void detachSyncEngine() {
        nativeDetachSyncEngine(handle);
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
            if (parts != null && parts.length >= 4) {
                notes.add(new NoteSummary(parts[0], parts[1], parts[2], parts[3]));
            }
        }
        return notes;
    }

    /** DK-07 S4: 设置笔记加密级别（fail-closed：非法级别/不存在 → false） */
    public boolean setNoteEncryption(String noteId, String level) {
        return nativeSetNoteEncryption(handle, noteId, level);
    }

    /** V20 Phase 3: 今日专注汇总（JSON 或 null） */
    public String todayFocusSummary() {
        return nativeTodayFocusSummary(handle);
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

    /** 时间机器：列出笔记快照元数据（JSON 数组）。失败返回 "[]"。 */
    public String listSnapshots(String noteId) {
        String s = nativeListSnapshots(handle, noteId);
        return s == null ? "[]" : s;
    }

    /** 时间机器：读取指定版本快照正文。失败返回 null。 */
    public String getSnapshotContent(String noteId, int version) {
        return nativeGetSnapshotContent(handle, noteId, version);
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
    private static native boolean nativeGetWifiOnly(long handle);
    private static native void nativeSetWifiOnly(long handle, boolean on);
    // DK-49：同步数据面七函数穿链（JSON 字符串返回；attach/detach 为装配辅助）
    private static native String nativeSyncStatus(long handle);
    private static native String nativeQueueList(long handle);
    private static native String nativeQueueDrain(long handle);
    private static native String nativeConflictList(long handle);
    private static native void nativeConflictResolve(long handle, String artifactId);
    private static native void nativeConflictResolveSemantic(long handle, String conflictId, String resolution);
    private static native void nativeAttachSyncEngine(long handle, long engineHandle);
    private static native void nativeDetachSyncEngine(long handle);
    // ------------------------------------------------------------------
    // DK-02 尾回收站 S2（Leo 对向复核补链 2026-10-29——与 Rust JNI export 对齐）
    // ------------------------------------------------------------------

    /** 回收站条目（软删笔记——deleted_at_ms 倒序）。 */
    public static class TrashedNoteItem {
        public final String noteId;
        public final long deletedAtMs;
        public final String title;
        /** 删除时路径快照（S1 旧标记无此字段=null）。 */
        public final String originPath;

        public TrashedNoteItem(String noteId, long deletedAtMs, String title, String originPath) {
            this.noteId = noteId;
            this.deletedAtMs = deletedAtMs;
            this.title = title;
            this.originPath = originPath;
        }
    }

    /** 列出回收站（软删笔记，deleted_at_ms 倒序;桥/域错误降级空列表）。 */
    public java.util.List<TrashedNoteItem> listTrashed() {
        String json = nativeListTrashed(handle);
        java.util.List<TrashedNoteItem> out = new java.util.ArrayList<>();
        if (json == null) return out;
        try {
            org.json.JSONArray arr = new org.json.JSONArray(json);
            for (int i = 0; i < arr.length(); i++) {
                org.json.JSONObject o = arr.getJSONObject(i);
                out.add(new TrashedNoteItem(
                        o.optString("noteId", ""),
                        o.optLong("deletedAtMs", 0L),
                        o.optString("title", ""),
                        o.has("originPath") && !o.isNull("originPath") ? o.optString("originPath") : null));
            }
        } catch (Exception ignored) {
            // 错误降级空列表（与域方法 warn 惯例一致）
        }
        return out;
    }

    /** 恢复笔记（软删标记移除;失败 false）。 */
    public boolean restoreNote(String noteId) {
        return nativeRestoreNote(handle, noteId);
    }

    /** 彻底删除（仅回收站内可删;失败 false）。 */
    public boolean purgeNote(String noteId) {
        return nativePurgeNote(handle, noteId);
    }

    /** 清理超期回收站（days 天前软删的永久删除）——返回清理条数。 */
    public int purgeExpired(long days) {
        return nativePurgeExpired(handle, days);
    }

    private static native String nativeListTrashed(long handle);
    private static native boolean nativeRestoreNote(long handle, String noteId);
    private static native boolean nativePurgeNote(long handle, String noteId);
    private static native int nativePurgeExpired(long handle, long days);
    private static native String nativeCreateNote(long handle, String title);
    private static native int nativeListNotesCount(long handle);
    private static native String[] nativeGetNote(long handle, int index);
    private static native int nativeSearchCount(long handle, String query);
    private static native Object[] nativeGetSearchResult(long handle, int index, String query);
    private static native int nativeDeleteNote(long handle, String noteId);
    private static native boolean nativeSetNoteEncryption(long handle, String noteId, String level);
    private static native String nativeTodayFocusSummary(long handle);
    private static native int nativeIsFallback(long handle);
    private static native int nativeSaveNoteContent(long handle, String noteId, String content);
    private static native String nativeGetNoteContent(long handle, String noteId);
    private static native String nativeGetNoteSnapshot(long handle, String noteId);
    private static native int nativeSaveNoteSnapshot(long handle, String noteId, String snapshotBase64);
    private static native String nativeListSnapshots(long handle, String noteId);
    private static native String nativeGetSnapshotContent(long handle, String noteId, int version);
    private static native void nativeDestroy(long handle);
}
