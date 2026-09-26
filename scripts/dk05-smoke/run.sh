#!/usr/bin/env bash
# DK-05 S2 冻结硬门槛 — Tauri 产物 + Xvfb 真机窗口冒烟（proot 辅助进程重定向版）
set -u
export LD_LIBRARY_PATH=/home/z/.local/pkg/root/usr/lib/x86_64-linux-gnu:/home/z/.local/pkg/root/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1:${LD_LIBRARY_PATH:-}
export GI_TYPELIB_PATH=/home/z/.local/pkg/root/usr/lib/x86_64-linux-gnu/girepository-1.0
export PATH=/home/z/.local/pkg/root/usr/bin:$PATH
BIN=/home/z/my-project/Aurora/target/debug/aurora-desktop
LOGDIR=/home/z/smoke/logs
mkdir -p $LOGDIR
NONCE="SMOKE$(date +%H%M%S)"
echo "NONCE=$NONCE" | tee $LOGDIR/nonce.txt

# 清理上次残留
pkill -f "Xvfb :77" 2>/dev/null; pkill -f aurora-desktop 2>/dev/null; sleep 1

# 1. Xvfb 真机显示
Xvfb :77 -screen 0 1280x800x24 > $LOGDIR/xvfb.log 2>&1 &
XVFB_PID=$!
sleep 2
export DISPLAY=:77

# 2. proot 包裹 app（WebKit helper 路径重定向到解包树）
proot -b /home/z/.local/pkg/root/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1:/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1 \
  $BIN > $LOGDIR/app.log 2>&1 &
APP_PID=$!
sleep 9

XDOOL=/home/z/.local/pkg/root/usr/bin/xdotool
WIN=$($XDOOL search --onlyvisible --name 'Aurora' 2>/dev/null | head -1)
[ -z "$WIN" ] && WIN=$($XDOOL search --onlyvisible --name '.' 2>/dev/null | grep -v "^$" | head -1)
echo "WIN=$WIN" | tee $LOGDIR/win.txt

if [ -n "$WIN" ]; then
  $XDOOL windowactivate --sync $WIN 2>/dev/null
  sleep 1
  # 3. 真实 X 键盘输入（点击正文区 → 键入 nonce）
  $XDOOL mousemove --sync 640 400 click 1 2>/dev/null
  sleep 1
  $XDOOL type --delay 55 "$NONCE hello tauri" 2>/dev/null
  sleep 5   # 防抖落库 1s + flush 余量
  # 4. 真实 X 滚轮并发输入（按钮 4/5 交替 8 轮）
  for i in 1 2 3 4 5 6 7 8; do $XDOOL click 4 2>/dev/null; $XDOOL click 5 2>/dev/null; done
  sleep 2
  echo "ACTIVE_WIN=$($XDOOL getactivewindowname 2>/dev/null)" | tee -a $LOGDIR/win.txt
  kill -0 $APP_PID 2>/dev/null && echo "RESULT: PROC_ALIVE" | tee -a $LOGDIR/win.txt || echo "RESULT: PROC_DEAD" | tee -a $LOGDIR/win.txt
else
  echo "RESULT: WINDOW_NOT_FOUND" | tee -a $LOGDIR/win.txt
fi

kill $APP_PID 2>/dev/null; kill $XVFB_PID 2>/dev/null; sleep 1

# 5. 数据级断言：nonce 落盘（真 invoke → Rust cmd → 存储）
echo "== data assertion" | tee -a $LOGDIR/nonce.txt
grep -rl "$NONCE" $HOME/.local/share/com.aurora.desktop $HOME/.config/com.aurora.desktop 2>/dev/null | head -3 | tee $LOGDIR/nonce_hit.txt
[ -s $LOGDIR/nonce_hit.txt ] && echo "NONCE_PERSISTED" || echo "NONCE_NOT_FOUND"
