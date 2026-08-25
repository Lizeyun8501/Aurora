package com.aurora.note;

/**
 * JNI bindings for P2P Sync Engine (aurora-mobile-ffi, feature: p2p-sync).
 *
 * V19 §31 DEV-005 — iroh QUIC 传输 + NAT 穿透，端点地址以 JSON 交换（QR/手动输入）。
 */
public class SyncEngine {

    private long engineHandle;

    /** 同步结果报告。 */
    public static class SyncReport {
        public final boolean success;
        public final long sentBytes;
        public final long receivedBytes;
        public final String remotePeer;
        public final String error;

        public SyncReport(boolean success, long sentBytes, long receivedBytes,
                          String remotePeer, String error) {
            this.success = success;
            this.sentBytes = sentBytes;
            this.receivedBytes = receivedBytes;
            this.remotePeer = remotePeer;
            this.error = error;
        }
    }

    private SyncEngine(long handle) {
        this.engineHandle = handle;
    }

    /**
     * 启动同步引擎：绑定 iroh Endpoint（QUIC + NAT 穿透）。
     * 失败返回 null（如无网络权限）。
     */
    public static SyncEngine start() {
        long handle = nativeStart(0);
        if (handle == 0) return null;
        return new SyncEngine(handle);
    }

    /** 本机端点地址（JSON 字符串，供对端连接/QR 展示）。 */
    public String localAddr() {
        if (engineHandle == 0) return null;
        return nativeLocalAddr(engineHandle);
    }

    /** 本机节点 ID（iroh EndpointId 十六进制，调试用）。 */
    public String nodeId() {
        return String.valueOf(engineHandle); // placeholder — node_id 经 localAddr JSON 提取
    }

    /**
     * 与对端同步指定笔记（客户端角色，阻塞直至完成）。
     *
     * @param peerAddrJson 对端 localAddr() 输出的 JSON
     * @param noteId 笔记 ID
     */
    public SyncReport syncNote(UniffiAppCore core, String peerAddrJson, String noteId) {
        if (engineHandle == 0) {
            return new SyncReport(false, 0, 0, "", "engine not started");
        }
        String[] r = nativeSyncNote(engineHandle, core.handle(), peerAddrJson, noteId);
        if (r == null || r.length < 5) {
            return new SyncReport(false, 0, 0, "", "native sync failed");
        }
        return new SyncReport(
                Boolean.parseBoolean(r[0]),
                Long.parseLong(r[1]),
                Long.parseLong(r[2]),
                r[3],
                r[4]);
    }

    /**
     * 启动指定笔记的接收循环（服务端角色，后台线程）。
     * 对端每次发起同步，本地自动合并并持久化。
     */
    public boolean startAcceptLoop(UniffiAppCore core, String noteId) {
        if (engineHandle == 0) return false;
        return nativeStartAccept(engineHandle, core.handle(), noteId) == 0;
    }

    /** 关闭引擎并释放 native 资源。 */
    public void close() {
        if (engineHandle != 0) {
            nativeClose(engineHandle);
            engineHandle = 0;
        }
    }

    @Override
    protected void finalize() throws Throwable {
        try { close(); } finally { super.finalize(); }
    }

    private static native long nativeStart(long coreHandle);
    private static native String nativeLocalAddr(long engineHandle);
    private static native String[] nativeSyncNote(long engineHandle, long coreHandle,
                                                  String peerAddrJson, String noteId);
    private static native int nativeStartAccept(long engineHandle, long coreHandle, String noteId);
    private static native void nativeClose(long engineHandle);
}
