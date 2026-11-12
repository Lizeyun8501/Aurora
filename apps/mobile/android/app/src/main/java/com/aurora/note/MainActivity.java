package com.aurora.note;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.os.Bundle;
import android.view.View;
import android.net.Uri;
import android.content.Intent;
import android.content.SharedPreferences;
import android.os.Environment;
import android.webkit.JavascriptInterface;
import android.provider.DocumentsContract;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.Toast;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.List;

/**
 * WebView 容器 — V19 §36.3 Capacitor+React 正式架构。
 *
 * 数据链路: React (assets/index.html)
 *         → window.AndroidBridge (@JavascriptInterface)
 *         → UniffiAppCore (JNI)
 *         → Rust aurora-mobile-ffi
 *         → aurora-bootstrap (SQLite + Tantivy + Crypto)
 */
public class MainActivity extends Activity {

    private WebView webView;
    private UniffiAppCore core;

    /** DK-08 §7.3：网络状态中继（ConnectivityManager → SyncGate） */
    private NetworkStateRelay networkRelay;
    /** P2P 同步引擎（懒启动，V19 §31 DEV-005） */
    private SyncEngine syncEngine;

    // ===== E25（TA 17:32）: 导出真文件落盘——SAF 目录选取+CREATE_DOCUMENT 直存 =====
    private static final int REQ_PICK_EXPORT_DIR = 7301;
    private static final int REQ_EXPORT_CREATE_DOC = 7302;
    private String pendingExportName;   // CREATE_DOCUMENT 挂起期文件名
    private String pendingExportMime;   // 挂起期 mime
    private String pendingExportText;   // 挂起期内容（JS 线程串来）

    // ── A-06: 系统分享/URL Scheme 接收队列（进程级静态——WebView 生命周期
    //    独立;JS 消费走 consumePendingIntents() 拉取,推拉解耦防丢） ──
    private static final java.util.ArrayDeque<String> pendingIntents = new java.util.ArrayDeque<>();
    private static int intentSeq = 0;
    private static volatile MainActivity instance;

    @SuppressLint({"SetJavaScriptEnabled", "AddJavascriptInterface"})
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        instance = this;

        // 初始化 Rust 核心（真实 bootstrap: SQLite + Tantivy）
        String dataDir = getFilesDir().getAbsolutePath();
        try {
            core = new UniffiAppCore(dataDir);
        } catch (Throwable e) {
            Toast.makeText(this, "核心初始化失败: " + e.getMessage(), Toast.LENGTH_LONG).show();
            finish();
            return;
        }

        // DK-08 §7.3：网络状态接线（registerDefaultNetworkCallback → JNI 推送）
        networkRelay = new NetworkStateRelay(this);
        networkRelay.register();

        // CALENDAR-04 提醒 P0（方案 B 裁决 10-30）: 启动重扫注册
        // （事件写后 JS 桥 refreshReminders() 增量补——WebView 死了也响）
        try {
            ReminderScheduler.ensureChannels(this);
            ReminderScheduler.reschedule(this, core);
        } catch (Throwable t) {
            android.util.Log.w("Aurora", "reminder rescan failed", t);
        }

        // A-06: 小组件数据通道（只读 core 引用——销毁置 null 降级）
        AuroraWidgetProvider.AuroraHolder.set(core);

        // A-06: 启动 intent 入队（冷启动分享/scheme——onCreate 只一次）
        try {
            enqueueIntent(getIntent());
        } catch (Throwable t) {
            android.util.Log.w("Aurora", "A-06 enqueue failed", t);
        }

        // WebView 容器
        webView = new WebView(this);
        webView.setLayoutParams(new FrameLayout.LayoutParams(
            FrameLayout.LayoutParams.MATCH_PARENT,
            FrameLayout.LayoutParams.MATCH_PARENT));

        WebSettings settings = webView.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setAllowFileAccess(false);
        settings.setAllowContentAccess(false);
        settings.setCacheMode(WebSettings.LOAD_DEFAULT);
        settings.setMediaPlaybackRequiresUserGesture(false);

        // JS bridge — React 侧通过 window.AndroidBridge 调用
        webView.addJavascriptInterface(new AndroidBridge(), "AndroidBridge");
        webView.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, String url) {
                // 只允许本地内容，阻止外部导航
                return !url.startsWith("file:///android_asset/");
            }
        });

        setContentView(webView);
        webView.loadUrl("file:///android_asset/index.html");
    }

    /**
     * JS Bridge — 暴露给 React 的原生接口。
     * 全部运行在 WebView JS 线程，JNI 调用本身是同步阻塞（毫秒级）。
     */
    private class AndroidBridge {

        @JavascriptInterface
        public String init(String dataDir) {
            return core != null ? "ok" : "error";
        }

        @JavascriptInterface
        public String listNotes() {
            try {
                List<UniffiAppCore.NoteSummary> notes = core.listNotes();
                JSONArray arr = new JSONArray();
                for (UniffiAppCore.NoteSummary n : notes) {
                    JSONObject o = new JSONObject();
                    o.put("id", n.id);
                    o.put("title", n.title);
                    o.put("updatedAt", n.updatedAt);
                    o.put("encryption", n.encryption);
                    arr.put(o);
                }
                return arr.toString();
            } catch (Exception e) {
                return "[]";
            }
        }

        @JavascriptInterface
        public boolean setNoteEncryption(String noteId, String level) {
            try {
                return core.setNoteEncryption(noteId, level);
            } catch (Exception e) {
                return false;
            }
        }

        /** DK-08 §7.3: 查询「仅 Wi-Fi 同步」（未初始化/异常恒 false = 不限制）。 */
        @JavascriptInterface
        public boolean getWifiOnly() {
            try {
                return core != null && core.getWifiOnly();
            } catch (Exception e) {
                return false;
            }
        }

        /** DK-08 §7.3: 设置「仅 Wi-Fi 同步」（KV 持久化 + 门即时切换）。 */
        @JavascriptInterface
        public void setWifiOnly(boolean on) {
            try {
                if (core != null) core.setWifiOnly(on);
            } catch (Exception e) {
                // 桥线程静默：失败值由下次 getWifiOnly 读取反映
            }
        }

        @JavascriptInterface
        public String todayFocusSummary() {
            try {
                return core.todayFocusSummary();
            } catch (Exception e) {
                return null;
            }
        }

        @JavascriptInterface
        public String createNote(String title) {
            try {
                return core.createNote(title);
            } catch (Exception e) {
                return null;
            }
        }

        @JavascriptInterface
        public String getNoteContent(String noteId) {
            try {
                return core.getNoteContent(noteId);
            } catch (Exception e) {
                return null;
            }
        }

        @JavascriptInterface
        public int saveNoteContent(String noteId, String content) {
            try {
                core.saveNoteContent(noteId, content);
                return 0;
            } catch (Exception e) {
                return -1;
            }
        }

        @JavascriptInterface
        public int deleteNote(String noteId) {
            try {
                core.deleteNote(noteId);
                return 0;
            } catch (Exception e) {
                return -1;
            }
        }

        @JavascriptInterface
        public String searchNotes(String query) {
            try {
                List<UniffiAppCore.SearchResult> results = core.searchNotes(query);
                JSONArray arr = new JSONArray();
                for (UniffiAppCore.SearchResult r : results) {
                    JSONObject o = new JSONObject();
                    o.put("noteId", r.noteId);
                    o.put("title", r.title);
                    o.put("snippet", r.snippet);
                    o.put("score", r.score);
                    arr.put(o);
                }
                return arr.toString();
            } catch (Exception e) {
                return "[]";
            }
        }

        @JavascriptInterface
        public boolean isFallback() {
            return core.isFallback();
        }

        // ------------------------------------------------------------------
        // DK-02 尾回收站 S2（JSON 串传递——searchNotes 惯例）
        // ------------------------------------------------------------------

        /** 回收站列表（deleted_at_ms 倒序）。JSON: [{noteId,title,deletedAtMs,originPath}] */
        @JavascriptInterface
        public String listTrashed() {
            try {
                List<UniffiAppCore.TrashedNoteItem> items = core.listTrashed();
                JSONArray arr = new JSONArray();
                for (UniffiAppCore.TrashedNoteItem t : items) {
                    JSONObject o = new JSONObject();
                    o.put("noteId", t.noteId);
                    o.put("title", t.title);
                    o.put("deletedAtMs", t.deletedAtMs);
                    o.put("originPath", t.originPath == null ? JSONObject.NULL : t.originPath);
                    arr.put(o);
                }
                return arr.toString();
            } catch (Exception e) {
                return "[]";
            }
        }

        /** 恢复笔记（软删标记移除）。 */
        @JavascriptInterface
        public boolean restoreNote(String noteId) {
            try {
                return core.restoreNote(noteId);
            } catch (Exception e) {
                return false;
            }
        }

        /** 彻底删除（仅回收站内可删——永久）。 */
        @JavascriptInterface
        public boolean purgeNote(String noteId) {
            try {
                return core.purgeNote(noteId);
            } catch (Exception e) {
                return false;
            }
        }

        /** 清理超期回收站（days 天前软删的永久删除）。返回清理数（-1=错误）。 */
        @JavascriptInterface
        public int purgeExpired(int days) {
            try {
                return core.purgeExpired(days);
            } catch (Exception e) {
                return -1;
            }
        }

        // ------------------------------------------------------------------
        // DK-CALENDAR-02: Event 四方法（JSON 串惯例——searchNotes 同模式）
        // ------------------------------------------------------------------

        /** 全量事件列表。JSON: [{id,title,start,end,all_day,location,note_id,color}] */
        @JavascriptInterface
        public String listEvents() {
            try {
                List<UniffiAppCore.EventItem> events = core.listEvents();
                JSONArray arr = new JSONArray();
                for (UniffiAppCore.EventItem ev : events) {
                    JSONObject o = new JSONObject();
                    o.put("id", ev.id);
                    o.put("title", ev.title);
                    o.put("start", ev.start);
                    o.put("end", ev.end == null ? JSONObject.NULL : ev.end);
                    o.put("all_day", ev.allDay);
                    o.put("location", ev.location == null ? JSONObject.NULL : ev.location);
                    o.put("note_id", ev.noteId == null ? JSONObject.NULL : ev.noteId);
                    o.put("color", ev.color);
                    arr.put(o);
                }
                return arr.toString();
            } catch (Exception e) {
                return "[]";
            }
        }

        /** 事件创建/更新二合一（全字段 JSON;id 空=新建）。成功返 event_id,失败空串。 */
        @JavascriptInterface
        public String upsertEvent(String eventJson) {
            try {
                return core.upsertEvent(eventJson);
            } catch (Exception e) {
                return "";
            }
        }

        /** 删除事件（幂等）。 */
        @JavascriptInterface
        public boolean deleteEvent(String eventId) {
            try {
                return core.deleteEvent(eventId);
            } catch (Exception e) {
                return false;
            }
        }

        /** CALENDAR-04: 事件写后提醒重扫（AlarmManager 滚动窗口补注册）。失败静默——启动兜底。 */
        @JavascriptInterface
        public boolean refreshReminders() {
            try {
                ReminderScheduler.reschedule(MainActivity.this, core);
                return true;
            } catch (Exception e) {
                return false;
            }
        }

        /** B-01: Agent 真实推理链（chat 失败=ERR: 前缀文案——UI 三态分流）。 */
        @JavascriptInterface
        public String agentSessionCreate() {
            try { return core != null ? core.agentSessionCreate() : ""; } catch (Exception e) { return ""; }
        }
        @JavascriptInterface
        public boolean agentSessionKill(String sessionId, String reason) {
            try { return core != null && core.agentSessionKill(sessionId, reason); } catch (Exception e) { return false; }
        }
        @JavascriptInterface
        public String agentSessionStatus(String sessionId) {
            try { return core != null ? core.agentSessionStatus(sessionId) : ""; } catch (Exception e) { return ""; }
        }
        @JavascriptInterface
        public String agentChat(String sessionId, String messagesJson) {
            try { return core != null ? core.agentChat(sessionId, messagesJson) : "ERR:CORE_NOT_INIT"; }
            catch (Exception e) { return "ERR:CHAT_FAILED:" + e.getMessage(); }
        }
        @JavascriptInterface
        public String aiLiquifyList() {
            try { return core != null ? core.aiLiquifyList() : ""; } catch (Exception e) { return ""; }
        }
        @JavascriptInterface
        public String aiLiquifyCommit(String proposalId, String selectedJson) {
            try { return core != null ? core.aiLiquifyCommit(proposalId, selectedJson) : "ERR:CORE_NOT_INIT"; }
            catch (Exception e) { return "ERR:COMMIT_FAILED:" + e.getMessage(); }
        }

        /** A-04: 书签四命令（与桌面同源;list 含死链态——禁静默隐藏）。 */
        @JavascriptInterface
        public String bookmarkAdd(String bookmarkJson) {
            try { return core != null ? core.bookmarkAdd(bookmarkJson) : "core 未初始化"; }
            catch (Exception e) { return "失败: " + e.getMessage(); }
        }
        @JavascriptInterface
        public String bookmarkList() {
            try { return core != null ? core.bookmarkList() : "[]"; }
            catch (Exception e) { return "[]"; }
        }
        @JavascriptInterface
        public String bookmarkRemove(String id) {
            try { return core != null ? core.bookmarkRemove(id) : "core 未初始化"; }
            catch (Exception e) { return "失败: " + e.getMessage(); }
        }
        @JavascriptInterface
        public String bookmarkReorder(String idsJson, String titlePatch) {
            try { return core != null ? core.bookmarkReorder(idsJson, titlePatch) : "core 未初始化"; }
            catch (Exception e) { return "失败: " + e.getMessage(); }
        }

        /** A-09: 高级搜索 JSON（5 运算符+徽章面——与桌面 cmd_search_advanced 同源）。 */
        /** A-06: 消费挂起 intent 队列（拉模式——JSON 数组一次性取走）。 */
        @JavascriptInterface
        public String consumePendingIntents() {
            org.json.JSONArray arr = new org.json.JSONArray();
            while (!pendingIntents.isEmpty()) {
                try { arr.put(new org.json.JSONObject(pendingIntents.pollFirst())); }
                catch (Throwable ignored) {}
            }
            return arr.toString();
        }

        /** A-06: 读分享图片为 base64（WebView file:// 禁用——data URL 预览通道）。 */
        @JavascriptInterface
        public String readSharedImage(String path) {
            try {
                java.io.File f = new java.io.File(path);
                if (!f.exists()) return "";
                byte[] buf = new byte[(int) f.length()];
                try (java.io.FileInputStream in = new java.io.FileInputStream(f)) {
                    int off = 0;
                    while (off < buf.length) {
                        int n = in.read(buf, off, buf.length - off);
                        if (n <= 0) break;
                        off += n;
                    }
                }
                return android.util.Base64.encodeToString(buf, android.util.Base64.NO_WRAP);
            } catch (Throwable t) {
                return "";
            }
        }

        /** A-06: 小组件手动刷新（JS 侧任务态变更后即时推——DoD <1s 通道二）。 */
        @JavascriptInterface
        public void refreshWidget() {
            try { AuroraWidgetProvider.pushUpdate(MainActivity.this, core); }
            catch (Throwable ignored) {}
        }

        /** A-05: 块引用解析 JSON（status 三态;anchor 空串=无锚点）。 */
        @JavascriptInterface
        public String resolveBlockRef(String noteId, String anchor) {
            try { return core != null ? core.blockRef(noteId, anchor) : "{\"status\":\"not_found\"}"; }
            catch (Exception e) { return "{\"status\":\"not_found\"}"; }
        }

        /** A-05: 面包屑 JSON（{id,title}[];失败空数组）。 */
        @JavascriptInterface
        public String breadcrumb(String noteId) {
            try { return core != null ? core.breadcrumb(noteId) : "[]"; }
            catch (Exception e) { return "[]"; }
        }

        @JavascriptInterface
        public String advancedSearch(String query) {
            try {
                return core != null ? core.advancedSearch(query) : "";
            } catch (Exception e) {
                return "";
            }
        }

        /** A-08: 任务全量镜像同步（双写面——移动 localStorage 权威,core 镜像供
         *  桌面日历同源消费;失败返空串,调用方 console.warn 非静默）。 */
        @JavascriptInterface
        public String taskSync(String tasksJson) {
            try {
                return core != null ? core.taskSync(tasksJson) : "";
            } catch (Exception e) {
                return "";
            }
        }

        /** A-08: CalendarVm JSON（events+tasks——移动日历切读入口,下卡切换）。 */
        @JavascriptInterface
        public String calendarVm() {
            try {
                return core != null ? core.calendarVm() : "";
            } catch (Exception e) {
                return "";
            }
        }

        /** A-08: 任务改期（TaskRescheduled 事件——投影/双端同步）。 */
        @JavascriptInterface
        public boolean taskReschedule(String taskId, String newDue) {
            try {
                return core != null && core.taskReschedule(taskId, newDue);
            } catch (Exception e) {
                return false;
            }
        }

        /** 范围查询 [fromMs, toMs)（交叠命中）。JSON 数组同 listEvents。 */
        @JavascriptInterface
        public String queryEvents(long fromMs, long toMs) {
            try {
                List<UniffiAppCore.EventItem> events = core.queryEvents(fromMs, toMs);
                JSONArray arr = new JSONArray();
                for (UniffiAppCore.EventItem ev : events) {
                    JSONObject o = new JSONObject();
                    o.put("id", ev.id);
                    o.put("title", ev.title);
                    o.put("start", ev.start);
                    o.put("end", ev.end == null ? JSONObject.NULL : ev.end);
                    o.put("all_day", ev.allDay);
                    o.put("location", ev.location == null ? JSONObject.NULL : ev.location);
                    o.put("note_id", ev.noteId == null ? JSONObject.NULL : ev.noteId);
                    o.put("color", ev.color);
                    arr.put(o);
                }
                return arr.toString();
            } catch (Exception e) {
                return "[]";
            }
        }

        // ------------------------------------------------------------------
        // ProseMirror + Loro 编辑器（V19 §35 DEV-009）
        // ------------------------------------------------------------------

        /** 获取笔记 Loro 快照（base64）— 编辑器初始化。失败返回 null。 */
        @JavascriptInterface
        public String getNoteSnapshot(String noteId) {
            try {
                return core.getNoteSnapshot(noteId);
            } catch (Throwable e) {
                return null;
            }
        }


        /** 保存 Loro 快照（base64，CRDT 合并）+ 持久化。 */
        @JavascriptInterface
        public boolean saveNoteSnapshot(String noteId, String snapshotBase64) {
            try {
                return core.saveNoteSnapshot(noteId, snapshotBase64);
            } catch (Throwable e) {
                return false;
            }
        }

        /** 时间机器：列出笔记快照元数据（JSON 数组）。 */
        @JavascriptInterface
        public String listSnapshots(String noteId) {
            try {
                return core.listSnapshots(noteId);
            } catch (Throwable e) {
                return "[]";
            }
        }

        /** 时间机器：读取指定版本快照正文。 */
        @JavascriptInterface
        public String getSnapshotContent(String noteId, int version) {
            try {
                return core.getSnapshotContent(noteId, version);
            } catch (Throwable e) {
                return null;
            }
        }

        // ------------------------------------------------------------------
        // P2P 同步（V19 §31 DEV-005 — iroh QUIC + NAT 穿透）
        // ------------------------------------------------------------------

        /** DK-49：引擎惰性启动 + 生命周期挂接（drain 补发通道就绪）。 */
        private void ensureSyncEngine() {
            if (syncEngine == null) {
                syncEngine = SyncEngine.start();
            }
            if (syncEngine != null && core != null) {
                try {
                    core.attachSyncEngine(syncEngine);
                } catch (Throwable ignored) {
                }
            }
        }

        /** 启动同步引擎，返回本机端点地址 JSON（供对端 QR/输入交换）。 */
        @JavascriptInterface
        public String startSyncEngine() {
            try {
                ensureSyncEngine();
                return syncEngine != null ? syncEngine.localAddr() : null;
            } catch (Throwable e) {
                return null;
            }
        }

        /** 获取本机端点地址 JSON（未启动时启动）。 */
        @JavascriptInterface
        public String getSyncLocalAddr() {
            try {
                ensureSyncEngine();
                return syncEngine != null ? syncEngine.localAddr() : null;
            } catch (Throwable e) {
                return null;
            }
        }

        /**
         * 与对端同步指定笔记。
         * @param peerAddrJson 对端端点地址 JSON
         * @return 结果 JSON {success, sentBytes, receivedBytes, remotePeer, error}
         */
        @JavascriptInterface
        public String syncNote(String peerAddrJson, String noteId) {
            try {
                ensureSyncEngine();
                if (syncEngine == null) return "{\"success\":false,\"error\":\"engine start failed\"}";
                SyncEngine.SyncReport r = syncEngine.syncNote(core, peerAddrJson, noteId);
                JSONObject o = new JSONObject();
                o.put("success", r.success);
                o.put("sentBytes", r.sentBytes);
                o.put("receivedBytes", r.receivedBytes);
                o.put("remotePeer", r.remotePeer);
                o.put("error", r.error == null ? "" : r.error);
                return o.toString();
            } catch (Throwable e) {
                return "{\"success\":false,\"error\":\"" + e.getMessage() + "\"}";
            }
        }

        /** 启动指定笔记的接收循环（服务端角色，对端发起时自动合并+持久化）。 */
        @JavascriptInterface
        public boolean startAcceptSync(String noteId) {
            try {
                ensureSyncEngine();
                return syncEngine != null && syncEngine.startAcceptLoop(core, noteId);
            } catch (Throwable e) {
                return false;
            }
        }

        /** 关闭同步引擎。 */
        @JavascriptInterface
        public void closeSyncEngine() {
            if (syncEngine != null) {
                syncEngine.close();
                syncEngine = null;
            }
            // DK-49：引擎生命周期注销 — 防 drain 走已 close 的 transport
            try {
                if (core != null) core.detachSyncEngine();
            } catch (Throwable ignored) {
            }
        }

        // ------------------------------------------------------------------
        // DK-49：移动端同步数据面（对齐桌面 sync_commands 七函数 —
        // JSON 透传；错误降级 null/"[]"/error json，UI 侧按空态/提示处理）
        // ------------------------------------------------------------------

        /** 函数 1：同步状态聚合 JSON（gate/wifi_only/queue_len/pending_*）。 */
        @JavascriptInterface
        public String syncStatus() {
            try {
                return core.syncStatus();
            } catch (Throwable e) {
                return null;
            }
        }

        /** 函数 2：离线队列明细 JSON 数组。 */
        @JavascriptInterface
        public String queueList() {
            try {
                return core.queueList();
            } catch (Throwable e) {
                return "[]";
            }
        }

        /** 函数 3：手动补发 JSON；前置不满足返回 error json（不静默）。 */
        @JavascriptInterface
        public String queueDrain() {
            try {
                return core.queueDrain();
            } catch (Throwable e) {
                return "{\"error\":\"" + e.getMessage() + "\"}";
            }
        }

        /** 函数 4：待处理冲突合并视图 JSON 数组。 */
        @JavascriptInterface
        public String conflictList() {
            try {
                return core.conflictList();
            } catch (Throwable e) {
                return "[]";
            }
        }

        /** 函数 5：真冲突副本闭环；失败返回 false。 */
        @JavascriptInterface
        public boolean conflictResolve(String artifactId) {
            try {
                core.conflictResolve(artifactId);
                return true;
            } catch (Throwable e) {
                return false;
            }
        }

        /** 函数 6：语义冲突解决；失败返回 false。 */
        @JavascriptInterface
        public boolean conflictResolveSemantic(String conflictId, String resolution) {
            try {
                core.conflictResolveSemantic(conflictId, resolution);
                return true;
            } catch (Throwable e) {
                return false;
            }
        }

        // ===== E25（TA 17:32）: 导出真文件——目录选取/默认位/写文件 =====

        /** 当前导出目录显示名（未选=Download/Aurora 默认说明;持久 SharedPreferences）。
         * 零 androidx: DocumentsContract tree id 尾段当目录名（SAF 惯例足够可读）。 */
        @JavascriptInterface
        public String getExportDirLabel() {
            SharedPreferences sp = getSharedPreferences("aurora", MODE_PRIVATE);
            String uri = sp.getString("export_dir_uri", "");
            if (uri == null || uri.isEmpty()) return "默认: Download/AuroraExports";
            try {
                String treeId = DocumentsContract.getTreeDocumentId(Uri.parse(uri));
                String name = treeId != null ? treeId.substring(treeId.lastIndexOf(':') + 1) : null;
                return name != null && !name.isEmpty() ? ("自定义: " + name) : "自定义目录";
            } catch (Throwable t) {
                return "自定义目录";
            }
        }

        /** 系统目录选取器（ACTION_OPEN_DOCUMENT_TREE）——结果经 onActivityResult 回推 JS。 */
        @JavascriptInterface
        public void pickExportDir() {
            Intent it = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            it.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
            startActivityForResult(it, REQ_PICK_EXPORT_DIR);
        }

        /** 清回默认目录。 */
        @JavascriptInterface
        public void resetExportDir() {
            getSharedPreferences("aurora", MODE_PRIVATE).edit().remove("export_dir_uri").apply();
        }

        /**
         * 真文件导出：选定自定义目录→DocumentsContract 直写;
         * 默认（未选）→Download/AuroraExports File API 写（app 免权限公共下载区）;
         * 兜底→ACTION_CREATE_DOCUMENT 系统另存面板。
         * 回调: window.__onExportDone(ok, path)。
         */
        @JavascriptInterface
        public void exportFile(final String filename, final String mime, final String text) {
            final String safe = filename == null ? "note.md" : filename.replaceAll("[\\/:*?\"<>|]", "_");
            SharedPreferences sp = getSharedPreferences("aurora", MODE_PRIVATE);
            String uri = sp.getString("export_dir_uri", "");
            if (uri != null && !uri.isEmpty()) {
                try {
                    Uri tree = Uri.parse(uri);
                    String docId = DocumentsContract.getTreeDocumentId(tree);
                    // 子目录: 自选树根直接建文件（display name 防重后缀自动（SAF 自动 (1) 处理可省——
                    // DocumentsContract 同名覆盖新内容,不合意;此处先探名防覆盖省读——直接建即以系统覆写语义为简）
                    Uri doc = DocumentsContract.createDocument(
                        getContentResolver(),
                        DocumentsContract.buildDocumentUriUsingTree(tree, docId),
                        mime != null ? mime : "text/plain",
                        safe);
                    if (doc != null) {
                        java.io.OutputStream os = getContentResolver().openOutputStream(doc);
                        if (os != null) {
                            os.write(text != null ? text.getBytes("UTF-8") : new byte[0]);
                            os.close();
                            notifyExportDone(true, safe);
                            return;
                        }
                    }
                } catch (Throwable t) { /* 降级到默认路径 */ }
            }
            // 默认: 公共 Download/AuroraExports（app 外可读——Environment API 免权限路径）
            try {
                java.io.File base = new java.io.File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "AuroraExports");
                if (!base.exists() && !base.mkdirs() && !base.exists()) throw new java.io.IOException("mkdirs fail");
                java.io.File out = uniqueFile(base, safe);
                java.io.FileOutputStream fos = new java.io.FileOutputStream(out);
                fos.write(text != null ? text.getBytes("UTF-8") : new byte[0]);
                fos.close();
                notifyExportDone(true, out.getAbsolutePath());
                return;
            } catch (Throwable t2) { /* 末路兜底 CREATE_DOCUMENT */ }
            pendingExportName = safe;
            pendingExportMime = mime;
            pendingExportText = text;
            Intent it = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            it.addCategory(Intent.CATEGORY_OPENABLE);
            it.setType(mime != null ? mime : "text/plain");
            it.putExtra(Intent.EXTRA_TITLE, safe);
            startActivityForResult(it, REQ_EXPORT_CREATE_DOC);
        }
    }

    /** E25: SAF 结果回推——目录选取持久化 / CREATE_DOCUMENT 写入。 */
    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == REQ_PICK_EXPORT_DIR) {
            if (resultCode == RESULT_OK && data != null && data.getData() != null) {
                Uri uri = data.getData();
                getContentResolver().takePersistableUriPermission(uri,
                    Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
                getSharedPreferences("aurora", MODE_PRIVATE).edit().putString("export_dir_uri", uri.toString()).apply();
                notifyExportDirPicked(true);
            } else {
                notifyExportDirPicked(false);
            }
            return;
        }
        if (requestCode == REQ_EXPORT_CREATE_DOC) {
            if (resultCode == RESULT_OK && data != null && data.getData() != null) {
                try {
                    java.io.OutputStream os = getContentResolver().openOutputStream(data.getData());
                    if (os != null) {
                        os.write(pendingExportText != null ? pendingExportText.getBytes("UTF-8") : new byte[0]);
                        os.close();
                        notifyExportDone(true, pendingExportName != null ? pendingExportName : "note");
                        return;
                    }
                } catch (Throwable t) { /* fallthrough fail */ }
            }
            notifyExportDone(false, null);
        }
    }

    private void notifyExportDone(final boolean ok, final String path) {
        final String esc = path == null ? "" : path.replace("\\", "/").replace("\"", "'");
        webView.post(() -> webView.evaluateJavascript(
            "window.__onExportDone && window.__onExportDone(" + ok + ", '" + esc + "')", null));
        runOnUiThread(() -> Toast.makeText(MainActivity.this,
            ok ? "已导出: " + (path != null ? path : "所选位置") : "导出失败", Toast.LENGTH_LONG).show());
    }

    private void notifyExportDirPicked(final boolean ok) {
        webView.post(() -> webView.evaluateJavascript(
            "window.__onExportDirPicked && window.__onExportDirPicked(" + ok + ")", null));
    }

    /** A-06: intent → JSON 入队（分享文本/图片/scheme 动作三型）。 */
    private static void enqueueIntent(Intent intent) {
        if (intent == null) return;
        String action = intent.getAction();
        if ("android.intent.action.SEND".equals(action)) {
            String type = intent.getType();
            String text = intent.getStringExtra(Intent.EXTRA_TEXT);
            String subject = intent.getStringExtra(Intent.EXTRA_SUBJECT);
            if (type != null && type.startsWith("image/")) {
                android.net.Uri uri = null;
                if (intent.getExtras() != null) {
                    try {
                        Object s = intent.getExtras().get(Intent.EXTRA_STREAM);
                        if (s instanceof android.net.Uri) uri = (android.net.Uri) s;
                        else if (s instanceof String) { try { uri = android.net.Uri.parse((String) s); } catch (Throwable ignored) {} }
                    } catch (Throwable ignored) {}
                }
                String path = uri != null ? saveSharedImage(uri) : null;
                if (path != null) {
                    pendingIntents.add(intentJson("image", path, type, null));
                } else {
                    pendingIntents.add(intentJson("text", "（图片接收失败——来源不可读）", type, null));
                }
            } else if (text != null && !text.isEmpty()) {
                pendingIntents.add(intentJson("text", text, type, subject));
            }
        } else if ("android.intent.action.VIEW".equals(action) && intent.getData() != null) {
            pendingIntents.add(intentJson("action", intent.getData().toString(), "x-scheme/aurora", null));
        }
    }

    /** A-06: intent JSON 形态（队列元素——JS consumePendingIntents 消费）。 */
    private static String intentJson(String kind, String content, String mime, String subject) {
        org.json.JSONObject o = new org.json.JSONObject();
        try {
            o.put("seq", ++intentSeq);
            o.put("kind", kind);
            o.put("content", content);
            o.put("mime", mime == null ? "" : mime);
            o.put("subject", subject == null ? "" : subject);
            o.put("at", System.currentTimeMillis());
        } catch (Throwable ignored) {}
        return o.toString();
    }

    /** A-06: 分享图片流拷贝到 files/shared/（content URI 读权限只在当刻——立即落盘）。 */
    private static String saveSharedImage(android.net.Uri uri) {
        try {
            java.io.File dir = new java.io.File(instance.getFilesDir(), "shared");
            if (!dir.exists() && !dir.mkdirs()) return null;
            String name = "share-" + System.currentTimeMillis() + ".png";
            java.io.File out = new java.io.File(dir, name);
            android.content.ContentResolver cr = instance.getContentResolver();
            try (java.io.InputStream in = cr.openInputStream(uri);
                 java.io.OutputStream os = new java.io.FileOutputStream(out)) {
                byte[] buf = new byte[8192];
                int n;
                while ((n = in.read(buf)) > 0) os.write(buf, 0, n);
            }
            return out.getAbsolutePath();
        } catch (Throwable t) {
            android.util.Log.w("Aurora", "A-06 image save failed", t);
            return null;
        }
    }

    /** 同名自动 (n) 序号防覆盖。 */
    private static java.io.File uniqueFile(java.io.File dir, String name) {
        java.io.File f = new java.io.File(dir, name);
        if (!f.exists()) return f;
        String base = name, ext = "";
        int dot = name.lastIndexOf('.');
        if (dot > 0) { base = name.substring(0, dot); ext = name.substring(dot); }
        for (int i = 1; i < 1000; i++) {
            f = new java.io.File(dir, base + "(" + i + ")" + ext);
            if (!f.exists()) return f;
        }
        return new java.io.File(dir, base + "-" + System.currentTimeMillis() + ext);
    }

    /** A-06: singleTask 已运行态分享/scheme 入口。 */
    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        try {
            enqueueIntent(intent);
            if (webView != null) {
                webView.evaluateJavascript(
                    "window.dispatchEvent(new CustomEvent('aurora:intent-received'))", null);
            }
        } catch (Throwable t) {
            android.util.Log.w("Aurora", "A-06 onNewIntent failed", t);
        }
    }

    /** A-06: 回前台刷新小组件（DoD <1s——onResume 直推 RemoteViews）。 */
    @Override
    protected void onResume() {
        super.onResume();
        try {
            AuroraWidgetProvider.pushUpdate(this, core);
        } catch (Throwable t) {
            android.util.Log.w("Aurora", "A-06 widget refresh failed", t);
        }
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();
        if (networkRelay != null) {
            networkRelay.unregister();
            networkRelay = null;
        }
        if (webView != null) {
            webView.destroy();
            webView = null;
        }
        if (core != null) {
            core.destroy();
            core = null;
        }
        AuroraWidgetProvider.AuroraHolder.set(null);
        if (syncEngine != null) {
            syncEngine.close();
            syncEngine = null;
        }
    }

    @Override
    public void onBackPressed() {
        if (webView != null && webView.canGoBack()) {
            webView.goBack();
        } else {
            super.onBackPressed();
        }
    }
}
