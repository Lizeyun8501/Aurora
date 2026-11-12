package com.aurora.note;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.widget.RemoteViews;

/**
 * A-06（TA 2026-11-12 工单）: 桌面小组件——今日待办数 + 一键快速捕获。
 *
 * 架构纪律（工单原文）:
 * - 数据走只读 VM 快照（core calendarVm JSON → 今日 due 任务计数）,
 *   **不启动 WebView**（RemoteViews 纯原生渲染——RemoteViews 面禁自绘容器）
 * - 刷新三通道: ①onUpdate（系统周期 30min+添加时）②MainActivity.onResume
 *   （回前台 <1s——DoD）③JS refreshWidget()（任务态变更即时推）
 * - 点击: 快速捕获 → aurora://capture;卡片主体 → aurora://today
 *   （经 MainActivity intent-filter 路由——复用 A-06 队列链）
 *
 * 诚实边界: 计数含今日到期未完成任务（done 排除）;core 未初始化=「—」。
 */
public final class AuroraWidgetProvider extends AppWidgetProvider {

    @Override
    public void onUpdate(Context ctx, AppWidgetManager mgr, int[] ids) {
        pushUpdate(ctx, AuroraHolder.core());
        super.onUpdate(ctx, mgr, ids);
    }

    /** 主动推（三通道共用——core 可为 null=降级「—」不崩）。 */
    public static void pushUpdate(Context ctx, UniffiAppCore core) {
        try {
            AppWidgetManager mgr = ctx.getSystemService(AppWidgetManager.class);
            if (mgr == null) return;
            RemoteViews views = new RemoteViews(ctx.getPackageName(), R.layout.widget_today);
            views.setTextViewText(R.id.widget_count, todayCount(core));

            // 快速捕获 → aurora://capture
            Intent cap = new Intent(Intent.ACTION_VIEW, android.net.Uri.parse("aurora://capture"));
            cap.setPackage(ctx.getPackageName());
            cap.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            PendingIntent pCap = PendingIntent.getActivity(
                ctx, 601, cap, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
            views.setOnClickPendingIntent(R.id.widget_capture, pCap);

            // 卡片主体 → aurora://today
            Intent today = new Intent(Intent.ACTION_VIEW, android.net.Uri.parse("aurora://today"));
            today.setPackage(ctx.getPackageName());
            today.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            PendingIntent pToday = PendingIntent.getActivity(
                ctx, 602, today, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
            views.setOnClickPendingIntent(R.id.widget_root, pToday);

            ComponentName cn = new ComponentName(ctx, AuroraWidgetProvider.class);
            mgr.updateAppWidget(cn, views);
        } catch (Throwable t) {
            android.util.Log.w("Aurora", "A-06 widget push failed", t);
        }
    }

    /** 今日 due 未完成任务计数（calendarVm 只读快照——零 WebView）。 */
    private static String todayCount(UniffiAppCore core) {
        if (core == null) return "—";
        try {
            String json = core.calendarVm();
            if (json == null || json.isEmpty()) return "0";
            org.json.JSONObject vm = new org.json.JSONObject(json);
            org.json.JSONArray tasks = vm.optJSONArray("tasks");
            if (tasks == null) return "0";
            String today = new java.text.SimpleDateFormat("yyyy-MM-dd", java.util.Locale.US)
                .format(new java.util.Date());
            int n = 0;
            for (int i = 0; i < tasks.length(); i++) {
                org.json.JSONObject t = tasks.optJSONObject(i);
                if (t == null) continue;
                String due = t.optString("due", "");
                String status = t.optString("status", "");
                if (due.startsWith(today) && !"done".equals(status)) n++;
            }
            return String.valueOf(n);
        } catch (Throwable t) {
            return "—";
        }
    }

    /** core 引用（MainActivity 生命周期内持有;销毁后 null——widget 降级不崩）。 */
    static final class AuroraHolder {
        private static volatile UniffiAppCore core;
        static UniffiAppCore core() { return core; }
        static void set(UniffiAppCore c) { core = c; }
    }
}
