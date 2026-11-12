package com.aurora.note;

import android.app.AlarmManager;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Notification;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.os.Build;

/**
 * A-06（TA 2026-11-12 工单）: 通知动作接收器——「延后 1 小时」。
 *
 * 纪律（工单原文）: 动作回写经 WritePath,禁止直接改 DB。
 * - 「延后 1 小时」= 纯闹钟重排（AlarmManager +1h 同 extras 复发通知）
 *   ——不触任何存储,零 DB 写 ✓
 * - 「完成」不经本接收器——走 PendingIntent 拉起 MainActivity
 *   aurora://task-done?taskId=… → JS 消费 → 既有任务更新流
 *   （localStorage 权威 + taskSync core 镜像 → 投影）= WritePath 语义 ✓
 */
public final class ReminderActionReceiver extends BroadcastReceiver {

    @Override
    public void onReceive(Context ctx, Intent intent) {
        if (!"aurora.action.SNOOZE_1H".equals(intent.getAction())) return;
        String title = intent.getStringExtra("title");
        int level = intent.getIntExtra("level", 0);
        String eventId = intent.getStringExtra("eventId");
        String taskId = intent.getStringExtra("taskId");
        if (title == null || title.isEmpty()) title = "日程提醒";

        long at = System.currentTimeMillis() + 60L * 60L * 1000L;
        AlarmManager am = ctx.getSystemService(AlarmManager.class);
        if (am == null) return;

        Intent fire = new Intent(ctx, ReminderReceiver.class);
        fire.putExtra("title", title);
        fire.putExtra("level", level);
        if (eventId != null) fire.putExtra("eventId", eventId);
        if (taskId != null) fire.putExtra("taskId", taskId);
        fire.putExtra("snoozed", true);
        // requestCode: taskId 优先（任务提醒重排覆盖）;事件回退
        int rc = (taskId != null ? taskId : (eventId != null ? eventId : "evt")).hashCode();
        PendingIntent pi = PendingIntent.getBroadcast(
            ctx, rc, fire, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        try {
            if (Build.VERSION.SDK_INT >= 31) {
                am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi);
            } else {
                am.setExact(AlarmManager.RTC_WAKEUP, at, pi);
            }
        } catch (SecurityException e) {
            // SCHEDULE_EXACT_ALARM 未授予——非精确窗降级（不静默丢）
            am.setWindow(AlarmManager.RTC_WAKEUP, at, 10L * 60L * 1000L, pi);
        }
        // 确认通知（轻提示——非提醒级）
        try {
            NotificationManager nm = ctx.getSystemService(NotificationManager.class);
            if (nm != null) {
                Notification.Builder b;
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    b = new Notification.Builder(ctx, ReminderScheduler.CHANNEL_EVENT);
                } else {
                    b = new Notification.Builder(ctx).setPriority(Notification.PRIORITY_LOW);
                }
                nm.notify(77000 + rc, b
                    .setSmallIcon(android.R.drawable.ic_popup_reminder)
                    .setContentTitle("已延后 1 小时")
                    .setContentText(title + " 将在 1 小时后再次提醒")
                    .setAutoCancel(true)
                    .build());
            }
        } catch (Throwable ignored) {}
    }
}
