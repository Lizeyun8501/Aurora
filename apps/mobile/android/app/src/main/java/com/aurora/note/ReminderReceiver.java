package com.aurora.note;

import android.app.Notification;
import android.app.NotificationManager;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.os.Build;

/**
 * CALENDAR-04 提醒触发接收器——闹钟到点 → 系统通知。
 *
 * vivo 语义: level 0=通知提醒（IMPORTANCE_HIGH 通知）;
 * level 1=闹钟级（同渠道+振动 pattern;全屏 Activity 挂后续卡
 * ——锁屏可读通知先行,诚实边界: 非全屏闹钟页）。
 *
 * 纯原生 Notification.Builder（项目零 androidx 依赖——gradle
 * dependencies {} 刻意空;不引 compat 包）。
 */
public final class ReminderReceiver extends BroadcastReceiver {
    private static int seq = 1;

    @Override
    public void onReceive(Context ctx, Intent intent) {
        String title = intent.getStringExtra("title");
        int level = intent.getIntExtra("level", 0);
        String eventId = intent.getStringExtra("eventId");
        if (title == null || title.isEmpty()) title = "日程提醒";
        int notifyId = (eventId == null ? "evt" : eventId).hashCode() + (seq++ & 0xFFFF);

        Notification.Builder b;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            String ch = level >= 1 ? ReminderScheduler.CHANNEL_EVENT_ALARM : ReminderScheduler.CHANNEL_EVENT;
            b = new Notification.Builder(ctx, ch);
        } else {
            // minSdk 24/25 无渠道——priority 顶格（ heads-up 惯例）
            b = new Notification.Builder(ctx);
            b.setPriority(Notification.PRIORITY_HIGH);
        }
        // A-06: 任务提醒（taskId 存在）挂双 action——完成走 app 拉起（WritePath 语义）,
        // 延后走 ReminderActionReceiver 纯闹钟重排（零 DB 写）。
        String taskId = intent.getStringExtra("taskId");
        boolean snoozed = intent.getBooleanExtra("snoozed", false);
        if (taskId != null && !taskId.isEmpty() && !snoozed) {
            // 完成 → aurora://task-done?taskId=…（拉起 app,JS 经任务更新流回写）
            android.content.Intent done = new android.content.Intent(
                android.content.Intent.ACTION_VIEW,
                android.net.Uri.parse("aurora://task-done?taskId=" + android.net.Uri.encode(taskId)));
            done.setPackage(ctx.getPackageName());
            done.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK);
            android.app.PendingIntent pDone = android.app.PendingIntent.getActivity(
                ctx, taskId.hashCode(), done,
                android.app.PendingIntent.FLAG_UPDATE_CURRENT | android.app.PendingIntent.FLAG_IMMUTABLE);
            b.addAction(0, "完成", pDone);

            // 延后 1 小时 → ReminderActionReceiver（纯闹钟重排）
            android.content.Intent snooze = new android.content.Intent(ctx, ReminderActionReceiver.class);
            snooze.setAction("aurora.action.SNOOZE_1H");
            snooze.putExtra("title", title);
            snooze.putExtra("level", level);
            if (eventId != null) snooze.putExtra("eventId", eventId);
            snooze.putExtra("taskId", taskId);
            android.app.PendingIntent pSnooze = android.app.PendingIntent.getBroadcast(
                ctx, taskId.hashCode() + 1, snooze,
                android.app.PendingIntent.FLAG_UPDATE_CURRENT | android.app.PendingIntent.FLAG_IMMUTABLE);
            b.addAction(0, "延后 1 小时", pSnooze);
        }

        Notification n = b
                .setSmallIcon(android.R.drawable.ic_popup_reminder)
                .setContentTitle(title)
                .setContentText(level >= 1 ? "日程即将开始（闹钟级提醒）" : "日程即将开始")
                .setCategory(Notification.CATEGORY_REMINDER)
                .setAutoCancel(true)
                .build();
        NotificationManager nm = ctx.getSystemService(NotificationManager.class);
        if (nm != null) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                if (ctx.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS)
                        == android.content.pm.PackageManager.PERMISSION_GRANTED) {
                    nm.notify(notifyId, n);
                }
            } else {
                nm.notify(notifyId, n);
            }
        }
    }
}
