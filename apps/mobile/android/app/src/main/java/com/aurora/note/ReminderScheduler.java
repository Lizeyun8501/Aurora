package com.aurora.note;

import android.app.AlarmManager;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import org.json.JSONArray;
import org.json.JSONObject;

/**
 * CALENDAR-04 提醒调度器（Leo 原生面——方案 B 裁决 2026-10-30 21:59:
 * 提醒升 P0,日历主承载待办/日程）。
 *
 * 职责: 启动/事件写后重扫 listEvents → 未来窗口内提醒规则展开
 * → AlarmManager 精确闹钟注册（setExactAndAllowWhileIdle——Doze 下
 * 仍准时;杀进程可响）。通知与全屏强提醒在 ReminderReceiver。
 *
 * 架构边界: 调度全原生（WebView 死了也响）;core 只做权威存储
 * （Event.reminders 字段 JSON 透传,FFI 零增量——listEvents 已含）。
 *
 * 诚实边界:
 * - 重复事件提醒只挂「未来窗口内可见实例」（月展开复用前端逻辑
 *   的简化版: Rust/TS 引擎不镜像到 Java——单次与近端重复实例覆盖,
 *   超远重复实例由下次启动/写后重扫补注册（滚动窗口语义）。
 * - SCHEDULE_EXACT_ALARM 权限 Android 12+ 需用户授予（缺省降
 *   setWindowAndAllowWhileIdle 非精确——不静默丢提醒）。
 */
public final class ReminderScheduler {
    /** 注册窗口（天）: 只注册未来 14 天内会响的提醒——滚动补扫。 */
    static final int WINDOW_DAYS = 14;
    /** 通知渠道 ID（ReminderReceiver 同源引用）。 */
    static final String CHANNEL_EVENT = "calendar_event";
    static final String CHANNEL_EVENT_ALARM = "calendar_event_alarm";

    private ReminderScheduler() {}

    /** 创建通知渠道（App 启动一次性,幂等）。 */
    static void ensureChannels(Context ctx) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return;
        NotificationManager nm = ctx.getSystemService(NotificationManager.class);
        if (nm == null) return;
        NotificationChannel c1 = new NotificationChannel(
                CHANNEL_EVENT, "日程提醒", NotificationManager.IMPORTANCE_HIGH);
        c1.setDescription("日程/待办到期提醒");
        nm.createNotificationChannel(c1);
        NotificationChannel c2 = new NotificationChannel(
                CHANNEL_EVENT_ALARM, "闹钟级提醒", NotificationManager.IMPORTANCE_HIGH);
        c2.setDescription("强提醒（振动+声音）");
        c2.enableVibration(true);
        nm.createNotificationChannel(c2);
    }

    /** 重扫+重注册（幂等: requestCode=eventId+offsetMin 唯一覆盖旧闹钟）。 */
    static void reschedule(Context ctx, UniffiAppCore core) {
        try {
            AlarmManager am = ctx.getSystemService(AlarmManager.class);
            if (am == null) return;
            String json = core.listEventsRaw(); // raw 串含 reminders（EventItem 强类型面丢字段——Bravo cfd0b32 解析层只取 UI 八字段）
            JSONArray events = new JSONArray(json == null || json.isEmpty() ? "[]" : json);
            long now = System.currentTimeMillis();
            long windowEnd = now + WINDOW_DAYS * 86_400_000L;
            for (int i = 0; i < events.length(); i++) {
                JSONObject ev = events.getJSONObject(i);
                JSONArray reminders = ev.optJSONArray("reminders");
                if (reminders == null || reminders.length() == 0) continue;
                long startMs = parseIso(ev.optString("start", ""));
                if (startMs <= 0) continue;
                String title = ev.optString("title", "日程");
                String id = ev.optString("id", "");
                for (int r = 0; r < reminders.length(); r++) {
                    JSONObject rule = reminders.getJSONObject(r);
                    long offsetMin = Math.max(0, rule.optLong("offset_min", 0));
                    int level = rule.optInt("alarm_level", 0);
                    long at = startMs - offsetMin * 60_000L;
                    if (at < now || at > windowEnd) continue; // 窗口外滚动补扫
                    scheduleOne(ctx, am, id, offsetMin, at, title, level);
                }
            }
            scheduleTodayTasks(ctx, am, core, now);
        } catch (Exception e) {
            android.util.Log.w("ReminderScheduler", "reschedule failed", e);
        }
    }

    /** A-06: 今日 due 任务提醒注册（calendarVm 只读快照——任务到点通知带
     *  taskId → ReminderReceiver 挂「完成/延后 1 小时」双 action）。
     *  窗口语义: 仅未来 24h 内 due 的未完成任务（重扫幂等覆盖）。 */
    private static void scheduleTodayTasks(Context ctx, AlarmManager am,
                                           UniffiAppCore core, long now) {
        try {
            String vmJson = core.calendarVm();
            if (vmJson == null || vmJson.isEmpty()) return;
            JSONObject vm = new JSONObject(vmJson);
            JSONArray tasks = vm.optJSONArray("tasks");
            if (tasks == null) return;
            java.text.SimpleDateFormat df = new java.text.SimpleDateFormat(
                "yyyy-MM-dd'T'HH:mm", java.util.Locale.US);
            java.util.TimeZone tz = java.util.TimeZone.getTimeZone("Asia/Shanghai");
            df.setTimeZone(tz);
            for (int i = 0; i < tasks.length(); i++) {
                JSONObject t = tasks.optJSONObject(i);
                if (t == null) continue;
                String status = t.optString("status", "");
                if ("done".equals(status)) continue;
                String taskId = t.optString("id", "");
                if (taskId.isEmpty()) continue;
                String title = t.optString("title", "待办提醒");
                String due = t.optString("due", "");
                if (due.length() < 16) continue; // 缺时分的 due（纯日期）不挂到点闹钟
                long dueMs;
                try { dueMs = df.parse(due.substring(0, 16)).getTime(); }
                catch (Exception pe) { continue; }
                if (dueMs <= now || dueMs > now + 86_400_000L) continue;
                scheduleOneTask(ctx, am, taskId, dueMs, title);
            }
        } catch (Exception e) {
            android.util.Log.w("ReminderScheduler", "A-06 task schedule failed", e);
        }
    }

    /** A-06: 任务提醒单点注册（requestCode=task:{id} 唯一——重扫覆盖）。 */
    private static void scheduleOneTask(Context ctx, AlarmManager am,
                                        String taskId, long at, String title) {
        Intent intent = new Intent(ctx, ReminderReceiver.class)
                .putExtra("taskId", taskId)
                .putExtra("title", title)
                .putExtra("level", 0);
        int requestCode = ("task:" + taskId).hashCode();
        PendingIntent pi = PendingIntent.getBroadcast(
                ctx, requestCode, intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        boolean canExact = Build.VERSION.SDK_INT < Build.VERSION_CODES.S
                || am.canScheduleExactAlarms();
        if (canExact) {
            am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi);
        } else {
            setWindowAndAllowWhileWhileIdleCompat(am, AlarmManager.RTC_WAKEUP, at, 10 * 60_000L, pi);
        }
    }

    static void scheduleOne(Context ctx, AlarmManager am, String eventId,
                            long offsetMin, long at, String title, int level) {
        Intent intent = new Intent(ctx, ReminderReceiver.class)
                .putExtra("eventId", eventId)
                .putExtra("title", title)
                .putExtra("level", level);
        // requestCode = (eventId,offsetMin) 唯一——重扫覆盖旧注册不叠闹钟
        int requestCode = (eventId + ":" + offsetMin).hashCode();
        PendingIntent pi = PendingIntent.getBroadcast(
                ctx, requestCode, intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        boolean canExact = Build.VERSION.SDK_INT < Build.VERSION_CODES.S
                || am.canScheduleExactAlarms();
        if (canExact) {
            am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi);
        } else {
            // 权限未授予: 窗口化降级（±10 分钟）——不静默丢提醒
            setWindowAndAllowWhileWhileIdleCompat(am, AlarmManager.RTC_WAKEUP, at, 10 * 60_000L, pi);
        }
    }

    /** setWindowAndAllowWhileWhileIdle 兼容包装（旧 API 无 whileIdle 变体）。 */
    private static void setWindowAndAllowWhileWhileIdleCompat(
            AlarmManager am, int type, long windowStart, long windowLength, PendingIntent pi) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            am.setAndAllowWhileIdle(type, windowStart, pi);
        } else {
            am.setWindow(type, windowStart, windowLength, pi);
        }
    }

    /** RFC3339 → epoch 毫秒（失败 0——start 坏事件跳过）。 */
    static long parseIso(String iso) {
        try {
            if (iso.endsWith("Z")) {
                java.time.Instant ins = java.time.Instant.parse(iso);
                return ins.toEpochMilli();
            }
            return java.time.OffsetDateTime.parse(iso).toInstant().toEpochMilli();
        } catch (Exception e) {
            return 0;
        }
    }
}
