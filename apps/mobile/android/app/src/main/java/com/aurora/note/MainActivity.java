package com.aurora.note;

import android.app.Activity;
import android.app.AlertDialog;
import android.graphics.Color;
import android.graphics.drawable.GradientDrawable;
import android.os.Bundle;
import android.text.Editable;
import android.text.TextWatcher;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.CheckBox;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;

/**
 * 主 Activity — V15 §4 移动端界面规范实现。
 *
 * §4.1 底部 5-Tab 导航: 笔记首页 ｜ AI 助手 ｜ 闪卡复习 ｜ 无限画布 ｜ 系统设置
 * §4.2 首页工作台: 顶部搜索框 + 待办角标 + FAB 悬浮按钮
 * §4.4 触控热区: 底部Tab 48dp, 按钮 44dp, FAB 56dp
 * §2.1 设计令牌: #6c8eef 品牌色, #0f1117 暗色背景
 */
public class MainActivity extends Activity {

    // V15 §2.1 设计令牌 — 色彩
    static final int COLOR_BG_PRIMARY = Color.parseColor("#FFFFFF");
    static final int COLOR_BG_SECONDARY = Color.parseColor("#F7F8FA");
    static final int COLOR_BG_TERTIARY = Color.parseColor("#EEF0F4");
    static final int COLOR_TEXT_PRIMARY = Color.parseColor("#1A1D27");
    static final int COLOR_TEXT_SECONDARY = Color.parseColor("#5B6171");
    static final int COLOR_TEXT_TERTIARY = Color.parseColor("#8B91A5");
    static final int COLOR_BRAND = Color.parseColor("#6C8EEF");
    static final int COLOR_SUCCESS = Color.parseColor("#5ED4A8");
    static final int COLOR_WARNING = Color.parseColor("#F0A35E");
    static final int COLOR_DANGER = Color.parseColor("#EF6C6C");
    static final int COLOR_INFO = Color.parseColor("#38BDF8");
    static final int COLOR_BORDER = Color.parseColor("#E8EAF0");

    // 暗色模式
    static final int DARK_BG_PRIMARY = Color.parseColor("#0F1117");
    static final int DARK_BG_SECONDARY = Color.parseColor("#1A1D27");
    static final int DARK_BG_TERTIARY = Color.parseColor("#232734");
    static final int DARK_TEXT_PRIMARY = Color.parseColor("#E8EAF0");
    static final int DARK_TEXT_SECONDARY = Color.parseColor("#8B91A5");
    static final int DARK_BORDER = Color.parseColor("#2E3340");

    // V15 §4.1 底部导航 Tab
    static final String TAB_NOTES = "notes";
    static final String TAB_AI = "ai";
    static final String TAB_FLASHCARD = "flashcard";
    static final String TAB_CANVAS = "canvas";
    static final String TAB_SETTINGS = "settings";

    // V15 §4.4 触控热区标准
    static final int TOUCH_TARGET_NAV = dp(48); // 底部导航
    static final int TOUCH_TARGET_BUTTON = dp(44); // 按钮
    static final int TOUCH_TARGET_FAB = dp(56); // FAB

    private UniffiAppCore core;
    private FrameLayout rootLayout;
    private LinearLayout contentArea;
    private LinearLayout bottomNav;
    private String currentTab = TAB_NOTES;
    private boolean darkMode = false;

    // 当前有效颜色（根据主题切换）
    private int bgPrimary, bgSecondary, bgTertiary, textPrimary, textSecondary, textTertiary, borderColor;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        // 初始化核心
        String dataDir = getFilesDir().getAbsolutePath();
        try {
            core = new UniffiAppCore(dataDir);
        } catch (Throwable e) {
            showErrorAndFinish("初始化失败", e.getMessage() != null ? e.getMessage() : e.toString());
            return;
        }

        // 初始化颜色
        updateThemeColors();

        // V15 §4.2 整体布局: 顶部搜索 + 内容区 + FAB + 底部导航
        rootLayout = new FrameLayout(this);
        rootLayout.setBackgroundColor(bgPrimary);

        LinearLayout mainColumn = new LinearLayout(this);
        mainColumn.setOrientation(LinearLayout.VERTICAL);
        mainColumn.setLayoutParams(new FrameLayout.LayoutParams(
            FrameLayout.LayoutParams.MATCH_PARENT,
            FrameLayout.LayoutParams.MATCH_PARENT
        ));

        // V15 §4.2 顶部全局搜索框 + 待办角标
        mainColumn.addView(buildTopBar());

        // 内容区（滚动）
        contentArea = new LinearLayout(this);
        contentArea.setOrientation(LinearLayout.VERTICAL);
        ScrollView scroll = new ScrollView(this);
        scroll.setLayoutParams(new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f
        ));
        scroll.addView(contentArea);
        mainColumn.addView(scroll);

        // V15 §4.1 底部 5-Tab 导航
        bottomNav = buildBottomNav();
        mainColumn.addView(bottomNav);

        rootLayout.addView(mainColumn);

        // V15 §4.2 FAB 悬浮按钮
        rootLayout.addView(buildFAB());

        setContentView(rootLayout);
        navigateTo(TAB_NOTES);
    }

    // V15 §4.2 顶部搜索栏 + 待办角标
    private View buildTopBar() {
        LinearLayout bar = new LinearLayout(this);
        bar.setOrientation(LinearLayout.HORIZONTAL);
        bar.setGravity(Gravity.CENTER_VERTICAL);
        bar.setPadding(dp(16), dp(12), dp(8), dp(12));
        bar.setBackgroundColor(bgSecondary);

        // 搜索框
        EditText search = new EditText(this);
        search.setHint("搜索笔记、任务...");
        search.setSingleLine(true);
        search.setBackgroundResource(0);
        search.setTextColor(textPrimary);
        search.setHintTextColor(textTertiary);
        search.setTextSize(14f);
        LinearLayout.LayoutParams sp = new LinearLayout.LayoutParams(0, dp(40), 1f);
        sp.rightMargin = dp(8);
        search.setLayoutParams(sp);
        search.addTextChangedListener(new TextWatcher() {
            @Override public void beforeTextChanged(CharSequence s, int st, int b, int c) {}
            @Override public void onTextChanged(CharSequence s, int st, int b, int c) {}
            @Override public void afterTextChanged(Editable s) {
                if (s.length() > 0) navigateToSearch(s.toString());
            }
        });
        bar.addView(search);

        // 待办角标
        TextView todoBadge = new TextView(this);
        todoBadge.setText("0");
        todoBadge.setTextColor(Color.WHITE);
        todoBadge.setTextSize(11f);
        todoBadge.setGravity(Gravity.CENTER);
        todoBadge.setBackground(makeCircleBg(COLOR_DANGER));
        todoBadge.setLayoutParams(new LinearLayout.LayoutParams(dp(24), dp(24)));
        bar.addView(todoBadge);

        return bar;
    }

    // V15 §4.1 底部 5-Tab 导航
    private LinearLayout buildBottomNav() {
        LinearLayout nav = new LinearLayout(this);
        nav.setOrientation(LinearLayout.HORIZONTAL);
        nav.setBackgroundColor(bgSecondary);
        nav.setPadding(0, dp(4), 0, dp(4));
        nav.setLayoutParams(new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT,
            LinearLayout.LayoutParams.WRAP_CONTENT
        ));

        // V15 §4.1: 笔记首页 ｜ AI 助手 ｜ 闪卡复习 ｜ 无限画布 ｜ 系统设置
        nav.addView(buildNavTab(TAB_NOTES, "📝", "笔记", true));
        nav.addView(buildNavTab(TAB_AI, "🤖", "AI", false));
        nav.addView(buildNavTab(TAB_FLASHCARD, "🎴", "闪卡", false));
        nav.addView(buildNavTab(TAB_CANVAS, "🎨", "画布", false));
        nav.addView(buildNavTab(TAB_SETTINGS, "⚙️", "设置", false));

        return nav;
    }

    // V15 §4.4: 底部导航 Tab 最小 48×48px
    private View buildNavTab(String tabId, String icon, String label, boolean active) {
        LinearLayout tab = new LinearLayout(this);
        tab.setOrientation(LinearLayout.VERTICAL);
        tab.setGravity(Gravity.CENTER);
        tab.setPadding(dp(4), dp(8), dp(4), dp(8));
        tab.setTag(tabId);
        tab.setLayoutParams(new LinearLayout.LayoutParams(0, TOUCH_TARGET_NAV + dp(16), 1f));

        TextView iconView = new TextView(this);
        iconView.setText(icon);
        iconView.setTextSize(20f);
        iconView.setGravity(Gravity.CENTER);
        iconView.setLayoutParams(new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, dp(28)
        ));
        tab.addView(iconView);

        TextView labelView = new TextView(this);
        labelView.setText(label);
        labelView.setTextSize(11f);
        labelView.setGravity(Gravity.CENTER);
        labelView.setTextColor(active ? COLOR_BRAND : textTertiary);
        labelView.setLayoutParams(new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, dp(16)
        ));
        tab.addView(labelView);

        // V15 §4.4 触控热区
        tab.setMinimumHeight(TOUCH_TARGET_NAV);
        tab.setOnClickListener(v -> navigateTo(tabId));

        return tab;
    }

    // V15 §4.2: FAB 悬浮按钮 56×56px
    private View buildFAB() {
        TextView fab = new TextView(this);
        fab.setText("+");
        fab.setTextColor(Color.WHITE);
        fab.setTextSize(24f);
        fab.setGravity(Gravity.CENTER);
        fab.setBackground(makeCircleBg(COLOR_BRAND));

        // V15 §4.4: FAB 56×56px, 右下角距边缘 16px, 距底部导航 16px
        FrameLayout.LayoutParams p = new FrameLayout.LayoutParams(TOUCH_TARGET_FAB, TOUCH_TARGET_FAB);
        p.gravity = Gravity.BOTTOM | Gravity.END;
        p.rightMargin = dp(16);
        p.bottomMargin = dp(72); // 底部导航高度 + 间距

        // V15 §2.7 阴影
        fab.setElevation(dp(6));
        fab.setLayoutParams(p);
        fab.setOnClickListener(v -> {
            try { core.createNote("New Note " + new SimpleDateFormat("HH:mm", Locale.getDefault()).format(new Date())); }
            catch (Exception e) { Toast.makeText(this, "创建失败: " + e.getMessage(), Toast.LENGTH_SHORT).show(); return; }
            navigateTo(TAB_NOTES);
            Toast.makeText(this, "笔记已创建", Toast.LENGTH_SHORT).show();
        });
        return fab;
    }

    // V15 §4.2 导航切换
    private void navigateTo(String tab) {
        currentTab = tab;
        contentArea.removeAllViews();
        updateNavHighlight();

        switch (tab) {
            case TAB_NOTES: renderNotesView(); break;
            case TAB_AI: renderAIView(); break;
            case TAB_FLASHCARD: renderFlashcardView(); break;
            case TAB_CANVAS: renderCanvasView(); break;
            case TAB_SETTINGS: renderSettingsView(); break;
        }
    }

    // V15 §3.1 + §7.2 笔记视图
    private void renderNotesView() {
        LinearLayout container = new LinearLayout(this);
        container.setOrientation(LinearLayout.VERTICAL);
        container.setPadding(dp(16), dp(16), dp(16), dp(16));

        try {
            var notes = core.listNotes();
            // 标题
            TextView header = new TextView(this);
            header.setText("笔记 (" + notes.size() + ")");
            header.setTextSize(18f);
            header.setTypeface(header.getTypeface(), android.graphics.Typeface.BOLD);
            header.setTextColor(textPrimary);
            header.setPadding(0, 0, 0, dp(12));
            container.addView(header);

            if (notes.isEmpty()) {
                // V15 §7.2 空状态
                container.addView(buildEmptyState("📝", "还没有笔记，点击 + 开始创建", "新建笔记"));
            } else {
                for (var n : notes) {
                    container.addView(buildNoteCard(n));
                }
            }
        } catch (Exception e) {
            container.addView(buildErrorState("加载失败: " + e.getMessage()));
        }
        contentArea.addView(container);
    }

    // 笔记卡片
    private View buildNoteCard(UniffiAppCore.NoteSummary n) {
        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setPadding(dp(16), dp(12), dp(16), dp(12));
        card.setBackgroundColor(bgSecondary);

        GradientDrawable bg = new GradientDrawable();
        bg.setColor(bgSecondary);
        bg.setCornerRadius(dp(10));
        bg.setStroke(1, COLOR_BORDER);
        card.setBackground(bg);

        LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        );
        p.bottomMargin = dp(8);
        card.setLayoutParams(p);

        // 点击打开编辑器
        card.setOnClickListener(v -> openNoteEditor(n.id, n.title));

        // 标题行
        LinearLayout titleRow = new LinearLayout(this);
        titleRow.setOrientation(LinearLayout.HORIZONTAL);
        titleRow.setGravity(Gravity.CENTER_VERTICAL);

        TextView title = new TextView(this);
        title.setText(n.title);
        title.setTextSize(15f);
        title.setTextColor(textPrimary);
        title.setTypeface(title.getTypeface(), android.graphics.Typeface.BOLD);
        title.setLayoutParams(new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));
        titleRow.addView(title);

        // 删除按钮
        TextView del = new TextView(this);
        del.setText("🗑");
        del.setTextSize(16f);
        del.setPadding(dp(12), dp(4), dp(0), dp(4));
        del.setMinimumWidth(TOUCH_TARGET_BUTTON);
        del.setMinimumHeight(TOUCH_TARGET_BUTTON);
        del.setGravity(Gravity.CENTER);
        del.setOnClickListener(v -> {
            new AlertDialog.Builder(this)
                .setTitle("删除笔记")
                .setMessage("确定删除「" + n.title + "」？")
                .setPositiveButton("删除", (d, w) -> {
                    try { core.deleteNote(n.id); navigateTo(TAB_NOTES); }
                    catch (Exception e) { Toast.makeText(this, "删除失败", Toast.LENGTH_SHORT).show(); }
                })
                .setNegativeButton("取消", null)
                .show();
        });
        titleRow.addView(del);
        card.addView(titleRow);

        // 时间 + ID
        TextView meta = new TextView(this);
        meta.setText(n.updatedAt + " · " + n.id.substring(0, Math.min(8, n.id.length())));
        meta.setTextSize(12f);
        meta.setTextColor(textTertiary);
        meta.setPadding(0, dp(4), 0, 0);
        card.addView(meta);

        return card;
    }

    // 笔记编辑器
    private void openNoteEditor(String noteId, String title) {
        ScrollView scroll = new ScrollView(this);
        LinearLayout container = new LinearLayout(this);
        container.setOrientation(LinearLayout.VERTICAL);
        container.setPadding(dp(16), dp(16), dp(16), dp(16));

        // 标题
        TextView titleLabel = new TextView(this);
        titleLabel.setText(title);
        titleLabel.setTextSize(20f);
        titleLabel.setTypeface(titleLabel.getTypeface(), android.graphics.Typeface.BOLD);
        titleLabel.setTextColor(textPrimary);
        container.addView(titleLabel);

        // 分隔线
        View divider = new View(this);
        divider.setBackgroundColor(COLOR_BORDER);
        LinearLayout.LayoutParams divParams = new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 1);
        divParams.topMargin = dp(12);
        container.addView(divider);

        // 内容编辑
        EditText editor = new EditText(this);
        editor.setHint("开始写作...");
        editor.setTextSize(16f);
        editor.setTextColor(textPrimary);
        editor.setBackgroundColor(Color.TRANSPARENT);
        editor.setMinLines(10);
        editor.setGravity(Gravity.TOP);
        editor.setPadding(0, dp(16), 0, dp(16));

        // 加载笔记内容
        try {
            String content = core.getNoteContent(noteId);
            editor.setText(content);
        } catch (Exception e) {
            editor.setText("");
        }

        LinearLayout.LayoutParams ep = new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        );
        ep.topMargin = dp(16);
        editor.setLayoutParams(ep);
        container.addView(editor);

        // 保存按钮
        Button saveBtn = new Button(this);
        saveBtn.setText("保存");
        saveBtn.setTextColor(Color.WHITE);
        saveBtn.setBackground(makeRoundRect(COLOR_BRAND, dp(8), COLOR_BRAND));
        saveBtn.setMinimumHeight(TOUCH_TARGET_BUTTON);
        saveBtn.setOnClickListener(v -> {
            try {
                core.saveNoteContent(noteId, editor.getText().toString());
                Toast.makeText(this, "已保存", Toast.LENGTH_SHORT).show();
                navigateTo(TAB_NOTES);
            } catch (Exception e) {
                Toast.makeText(this, "保存失败: " + e.getMessage(), Toast.LENGTH_SHORT).show();
            }
        });
        container.addView(saveBtn);

        scroll.addView(container);
        contentArea.removeAllViews();
        contentArea.addView(scroll);
    }

    // V15 §3.8 AI 助手视图
    private void renderAIView() {
        LinearLayout container = new LinearLayout(this);
        container.setOrientation(LinearLayout.VERTICAL);
        container.setPadding(dp(16), dp(16), dp(16), dp(16));

        // 标题
        TextView header = new TextView(this);
        header.setText("🤖 AI 助手");
        header.setTextSize(18f);
        header.setTypeface(header.getTypeface(), android.graphics.Typeface.BOLD);
        header.setTextColor(textPrimary);
        container.addView(header);

        // 推理模式指示器
        TextView mode = new TextView(this);
        mode.setText("● 本地模型 (Ollama)");
        mode.setTextSize(12f);
        mode.setTextColor(COLOR_SUCCESS);
        mode.setPadding(0, dp(4), 0, dp(16));
        container.addView(mode);

        // V15 §3.16 权限标识
        TextView perm = new TextView(this);
        perm.setText("基于你有权限的笔记回答");
        perm.setTextSize(11f);
        perm.setTextColor(textTertiary);
        perm.setPadding(0, dp(4), 0, dp(16));
        container.addView(perm);

        // 对话区域
        TextView chat = new TextView(this);
        chat.setText("问我任何关于你的知识库的问题...\n\n例如：\n• 总结我最近的笔记\n• 这个标签下有哪些待办？\n• 帮我写一个会议纪要模板");
        chat.setTextSize(14f);
        chat.setTextColor(textSecondary);
        chat.setLineSpacing(0, 1.6f);
        chat.setPadding(dp(16), dp(16), dp(16), dp(16));

        GradientDrawable chatBg = new GradientDrawable();
        chatBg.setColor(bgTertiary);
        chatBg.setCornerRadius(dp(10));
        chat.setBackground(chatBg);
        chat.setLayoutParams(new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ));
        container.addView(chat);

        // 输入框
        EditText input = new EditText(this);
        input.setHint("输入问题...");
        input.setHintTextColor(textTertiary);
        input.setTextColor(textPrimary);
        input.setTextSize(14f);
        LinearLayout.LayoutParams ip = new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, dp(48)
        );
        ip.topMargin = dp(16);
        input.setLayoutParams(ip);
        input.setBackground(makeRoundRect(bgSecondary, dp(8), COLOR_BORDER));
        input.setPadding(dp(12), dp(8), dp(12), dp(8));
        container.addView(input);

        contentArea.addView(container);
    }

    // V15 §3.5 闪卡复习视图
    private void renderFlashcardView() {
        LinearLayout container = new LinearLayout(this);
        container.setOrientation(LinearLayout.VERTICAL);
        container.setPadding(dp(16), dp(16), dp(16), dp(16));

        TextView header = new TextView(this);
        header.setText("🎴 闪卡复习");
        header.setTextSize(18f);
        header.setTypeface(header.getTypeface(), android.graphics.Typeface.BOLD);
        header.setTextColor(textPrimary);
        container.addView(header);

        // 今日待复习数
        TextView count = new TextView(this);
        count.setText("0");
        count.setTextSize(48f);
        count.setTextColor(COLOR_SUCCESS);
        count.setTypeface(count.getTypeface(), android.graphics.Typeface.BOLD);
        count.setGravity(Gravity.CENTER);
        count.setPadding(0, dp(24), 0, dp(8));
        container.addView(count);

        TextView sub = new TextView(this);
        sub.setText("今日待复习");
        sub.setTextSize(14f);
        sub.setTextColor(textSecondary);
        sub.setGravity(Gravity.CENTER);
        container.addView(sub);

        // 空状态
        container.addView(buildEmptyState("🎴", "今日复习已完成！\n或还没有闪卡", "从笔记创建闪卡"));
    }

    // V15 §3.4.4 无限画布视图
    private void renderCanvasView() {
        LinearLayout container = new LinearLayout(this);
        container.setOrientation(LinearLayout.VERTICAL);
        container.setPadding(dp(16), dp(16), dp(16), dp(16));

        TextView header = new TextView(this);
        header.setText("🎨 无限画布");
        header.setTextSize(18f);
        header.setTypeface(header.getTypeface(), android.graphics.Typeface.BOLD);
        header.setTextColor(textPrimary);
        container.addView(header);

        // 工具栏
        LinearLayout toolbar = new LinearLayout(this);
        toolbar.setOrientation(LinearLayout.HORIZONTAL);
        toolbar.setPadding(0, dp(16), 0, dp(16));
        String[] tools = {"📝", "📄", "🖼", "🔗", "📐"};
        for (String t : tools) {
            TextView btn = new TextView(this);
            btn.setText(t);
            btn.setTextSize(20f);
            btn.setGravity(Gravity.CENTER);
            btn.setMinimumWidth(TOUCH_TARGET_BUTTON);
            btn.setMinimumHeight(TOUCH_TARGET_BUTTON);
            btn.setBackground(makeRoundRect(bgTertiary, dp(8), COLOR_BORDER));
            btn.setPadding(dp(8), dp(8), dp(8), dp(8));
            toolbar.addView(btn);
        }
        container.addView(toolbar);

        // 画布区域（空状态）
        container.addView(buildEmptyState("🎨", "空白画布，拖入文档或图片开始创作", "选择模板"));
    }

    // V15 §3.7 + §3.9 设置视图
    private void renderSettingsView() {
        LinearLayout container = new LinearLayout(this);
        container.setOrientation(LinearLayout.VERTICAL);
        container.setPadding(dp(16), dp(16), dp(16), dp(16));

        TextView header = new TextView(this);
        header.setText("⚙️ 系统设置");
        header.setTextSize(18f);
        header.setTypeface(header.getTypeface(), android.graphics.Typeface.BOLD);
        header.setTextColor(textPrimary);
        header.setPadding(0, 0, 0, dp(16));
        container.addView(header);

        // 主题切换
        View themeRow = buildSettingRow("外观", darkMode ? "暗色" : "浅色");
        themeRow.setOnClickListener(v -> {
            darkMode = !darkMode;
            updateThemeColors();
            navigateTo(TAB_SETTINGS);
        });
        container.addView(themeRow);

        // 同步状态
        container.addView(buildSettingRow("同步状态", core.isFallback() ? "内存模式" : "已同步"));
        container.addView(buildSettingRow("加密", "E2EE 已启用"));
        container.addView(buildSettingRow("AI 模型", "Ollama (本地)"));
        container.addView(buildSettingRow("存储路径", getFilesDir().getAbsolutePath()));
        container.addView(buildSettingRow("版本", "0.1.0 (V15)"));

        // V15 §4.1 底部导航自定义
        container.addView(buildSettingRow("导航自定义", "5 个 Tab >"));

        contentArea.addView(container);
    }

    private View buildSettingRow(String label, String value) {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(Gravity.CENTER_VERTICAL);
        row.setPadding(dp(16), dp(14), dp(16), dp(14));
        row.setBackground(makeRoundRect(bgSecondary, dp(8), COLOR_BORDER));

        TextView l = new TextView(this);
        l.setText(label);
        l.setTextSize(14f);
        l.setTextColor(textPrimary);
        l.setLayoutParams(new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));
        row.addView(l);

        TextView v = new TextView(this);
        v.setText(value);
        v.setTextSize(13f);
        v.setTextColor(textTertiary);
        row.addView(v);

        LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        );
        p.bottomMargin = dp(8);
        row.setLayoutParams(p);
        row.setMinimumHeight(TOUCH_TARGET_BUTTON);
        return row;
    }

    // V15 §7.2 空状态
    private View buildEmptyState(String icon, String message, String action) {
        LinearLayout empty = new LinearLayout(this);
        empty.setOrientation(LinearLayout.VERTICAL);
        empty.setGravity(Gravity.CENTER);
        empty.setPadding(dp(32), dp(48), dp(32), dp(48));

        TextView iconView = new TextView(this);
        iconView.setText(icon);
        iconView.setTextSize(48f);
        iconView.setGravity(Gravity.CENTER);
        iconView.setPadding(0, 0, 0, dp(16));
        empty.addView(iconView);

        TextView msg = new TextView(this);
        msg.setText(message);
        msg.setTextSize(14f);
        msg.setTextColor(textTertiary);
        msg.setGravity(Gravity.CENTER);
        msg.setLineSpacing(0, 1.6f);
        empty.addView(msg);

        if (action != null) {
            TextView btn = new TextView(this);
            btn.setText(action);
            btn.setTextSize(14f);
            btn.setTextColor(COLOR_BRAND);
            btn.setGravity(Gravity.CENTER);
            btn.setPadding(dp(24), dp(12), dp(24), dp(12));
            btn.setBackground(makeRoundRect(bgTertiary, dp(8), COLOR_BRAND));
            LinearLayout.LayoutParams bp = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.WRAP_CONTENT, TOUCH_TARGET_BUTTON
            );
            bp.topMargin = dp(16);
            btn.setLayoutParams(bp);
            empty.addView(btn);
        }

        return empty;
    }

    private View buildErrorState(String message) {
        TextView err = new TextView(this);
        err.setText("⚠ " + message);
        err.setTextColor(COLOR_DANGER);
        err.setTextSize(14f);
        err.setPadding(dp(16), dp(16), dp(16), dp(16));
        return err;
    }

    private void navigateToSearch(String query) {
        contentArea.removeAllViews();
        LinearLayout container = new LinearLayout(this);
        container.setOrientation(LinearLayout.VERTICAL);
        container.setPadding(dp(16), dp(16), dp(16), dp(16));

        try {
            var results = core.searchNotes(query);
            TextView header = new TextView(this);
            header.setText("搜索 \"" + query + "\" (" + results.size() + " 结果)");
            header.setTextSize(15f);
            header.setTextColor(textPrimary);
            header.setTypeface(header.getTypeface(), android.graphics.Typeface.BOLD);
            header.setPadding(0, 0, 0, dp(12));
            container.addView(header);

            if (results.isEmpty()) {
                container.addView(buildEmptyState("🔍", "未找到相关结果，试试其他关键词", null));
            } else {
                for (var r : results) {
                    LinearLayout card = new LinearLayout(this);
                    card.setOrientation(LinearLayout.VERTICAL);
                    card.setPadding(dp(16), dp(12), dp(16), dp(12));
                    card.setBackground(makeRoundRect(bgSecondary, dp(10), COLOR_BORDER));
                    LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(
                        LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
                    );
                    p.bottomMargin = dp(8);
                    card.setLayoutParams(p);

                    TextView title = new TextView(this);
                    title.setText(r.title);
                    title.setTextSize(14f);
                    title.setTextColor(textPrimary);
                    title.setTypeface(title.getTypeface(), android.graphics.Typeface.BOLD);
                    card.addView(title);

                    if (r.snippet != null && !r.snippet.isEmpty()) {
                        TextView snip = new TextView(this);
                        snip.setText(r.snippet);
                        snip.setTextSize(13f);
                        snip.setTextColor(textSecondary);
                        snip.setPadding(0, dp(4), 0, 0);
                        card.addView(snip);
                    }

                    container.addView(card);
                }
            }
        } catch (Exception e) {
            container.addView(buildErrorState("搜索失败: " + e.getMessage()));
        }
        contentArea.addView(container);
    }

    // 底部导航高亮
    private void updateNavHighlight() {
        for (int i = 0; i < bottomNav.getChildCount(); i++) {
            View child = bottomNav.getChildAt(i);
            if (child instanceof LinearLayout) {
                LinearLayout tab = (LinearLayout) child;
                Object tag = tab.getTag();
                String tabId = tag != null ? tag.toString() : "";
                if (tab.getChildCount() > 1 && tab.getChildAt(1) instanceof TextView) {
                    TextView label = (TextView) tab.getChildAt(1);
                    label.setTextColor(tabId.equals(currentTab) ? COLOR_BRAND : textTertiary);
                }
            }
        }
    }

    private void updateThemeColors() {
        if (darkMode) {
            bgPrimary = DARK_BG_PRIMARY;
            bgSecondary = DARK_BG_SECONDARY;
            bgTertiary = DARK_BG_TERTIARY;
            textPrimary = DARK_TEXT_PRIMARY;
            textSecondary = DARK_TEXT_SECONDARY;
            textTertiary = DARK_TEXT_SECONDARY;
            borderColor = DARK_BORDER;
        } else {
            bgPrimary = COLOR_BG_PRIMARY;
            bgSecondary = COLOR_BG_SECONDARY;
            bgTertiary = COLOR_BG_TERTIARY;
            textPrimary = COLOR_TEXT_PRIMARY;
            textSecondary = COLOR_TEXT_SECONDARY;
            textTertiary = COLOR_TEXT_TERTIARY;
            borderColor = COLOR_BORDER;
        }
    }

    // 工具方法
    private static int dp(int v) { return (int)(v * android.content.res.Resources.getSystem().getDisplayMetrics().density); }

    private GradientDrawable makeCircleBg(int color) {
        GradientDrawable d = new GradientDrawable();
        d.setShape(GradientDrawable.OVAL);
        d.setColor(color);
        return d;
    }

    private GradientDrawable makeRoundRect(int bgColor, int radius, int stroke) {
        GradientDrawable d = new GradientDrawable();
        d.setColor(bgColor);
        d.setCornerRadius(radius);
        d.setStroke(1, stroke);
        return d;
    }

    private void showErrorAndFinish(String title, String message) {
        new AlertDialog.Builder(this)
            .setTitle(title)
            .setMessage(message)
            .setCancelable(false)
            .setPositiveButton("退出", (d, w) -> finish())
            .show();
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();
        if (core != null) { core.destroy(); core = null; }
    }

    @Override
    public void onBackPressed() {
        if (!TAB_NOTES.equals(currentTab)) {
            navigateTo(TAB_NOTES);
        } else {
            super.onBackPressed();
        }
    }
}
