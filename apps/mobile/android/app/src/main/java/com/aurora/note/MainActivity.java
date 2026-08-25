package com.aurora.note;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.os.Bundle;
import android.view.View;
import android.webkit.JavascriptInterface;
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
    /** P2P 同步引擎（懒启动，V19 §31 DEV-005） */
    private SyncEngine syncEngine;

    @SuppressLint({"SetJavaScriptEnabled", "AddJavascriptInterface"})
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        // 初始化 Rust 核心（真实 bootstrap: SQLite + Tantivy）
        String dataDir = getFilesDir().getAbsolutePath();
        try {
            core = new UniffiAppCore(dataDir);
        } catch (Throwable e) {
            Toast.makeText(this, "核心初始化失败: " + e.getMessage(), Toast.LENGTH_LONG).show();
            finish();
            return;
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
                    arr.put(o);
                }
                return arr.toString();
            } catch (Exception e) {
                return "[]";
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
        // P2P 同步（V19 §31 DEV-005 — iroh QUIC + NAT 穿透）
        // ------------------------------------------------------------------

        /** 启动同步引擎，返回本机端点地址 JSON（供对端 QR/输入交换）。 */
        @JavascriptInterface
        public String startSyncEngine() {
            try {
                if (syncEngine == null) {
                    syncEngine = SyncEngine.start();
                }
                return syncEngine != null ? syncEngine.localAddr() : null;
            } catch (Throwable e) {
                return null;
            }
        }

        /** 获取本机端点地址 JSON（未启动时启动）。 */
        @JavascriptInterface
        public String getSyncLocalAddr() {
            try {
                if (syncEngine == null) {
                    syncEngine = SyncEngine.start();
                }
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
                if (syncEngine == null) {
                    syncEngine = SyncEngine.start();
                }
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
                if (syncEngine == null) {
                    syncEngine = SyncEngine.start();
                }
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
        }
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();
        if (webView != null) {
            webView.destroy();
            webView = null;
        }
        if (core != null) {
            core.destroy();
            core = null;
        }
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
