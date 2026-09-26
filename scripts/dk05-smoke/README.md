# DK-05 桌面冒烟基建（S2 冻结硬门槛配套）

> 沉淀自 Bravo 2026-09-26 冒烟执行（回执 issues/wf/alpha-DK05-desktop-editor.md · 1d85ee6）。
> 用途：持桌面环境者按 docs/DK05-S2-smoke-checklist.md 执行真机冒烟时复用。

## 资产

- `xvfb_smoke.js` — 无 GPU 云环境冒烟（7 断言：X 会话窗口/invoke 回落/编辑器挂载/原生 X 键盘输入 nonce 回显/滚轮并发/焦点链/零崩溃）。Chromium 引擎，CDP attach 模式。
- `run.sh` — Tauri 产物 + proot 版编排（WebKitGTK 路径重定向；注意：无 GPU 环境下 WebKitGTK 渲染管线栈级不可运行——MiniBrowser 同 crashed，ANGLE 硬依赖 GPU EGL，留作有 GPU 环境复用）。

## 环境依赖（user-space 复刻路径，无 sudo）

1. `apt-get download` + `dpkg -x` 解包 280+ 包至 `~/.local/pkg/root`（webkit2gtk-4.1-dev/GTK3/mesa/gstreamer/xauth/x11-utils/libxdo3/proot/libtalloc2）
2. `PKG_CONFIG_PATH=~/.local/pkg/root/usr/lib/x86_64-linux-gnu/pkgconfig:...`
3. 链接：`cargo rustc -p aurora-desktop --bin aurora-desktop -- -L native=~/.local/pkg/root/usr/lib/x86_64-linux-gnu`
4. trixie t64 陷阱：`libgtk-3-0` 已更名 `libgtk-3-0t64`——dev 软链（libgtk-3.so）须指向系统 `/usr/lib/x86_64-linux-gnu/libgtk-3.so.0`

## 用法（xvfb_smoke.js）

```bash
Xvfb :77 -screen 0 1280x800x24 &
~/.cache/ms-playwright/chromium-*/chrome-linux64/chrome --no-sandbox --disable-gpu \
  --remote-debugging-port=9222 --user-data-dir=/tmp/cprof --window-size=1280,800 about:blank &  # DISPLAY=:77
xdotool windowsize <win_id> 1280 800   # 无 WM 时需手动设窗口
node xvfb_smoke.js                      # 期望 7/7 PASS
```

## 已知差距（真机触点关闭）

- IME composition 段（Xvfb 无 IME 引擎）
- WKWebView/WebView2 引擎差异
