#!/usr/bin/env bash
# ============================================================================
# Aurora Note 一键构建（Linux/macOS）
#
# 用法:
#   ./scripts/build.sh                 # Debug APK（默认）
#   ./scripts/build.sh --release       # Release APK
#   ./scripts/build.sh --skip-frontend # 跳过前端（复用 dist/）
#   ./scripts/build.sh --clean         # 清理 FFI 产物后重编
#   ./scripts/build.sh --check         # 仅环境自检
#
# 链路: 前端 vite → FFI 交叉编译(NDK) → jniLibs → gradle APK
# 产物: apps/mobile/android/app/build/outputs/apk/{debug|release}/
# ============================================================================
set -euo pipefail

# ---------- 参数 ----------
MODE="debug"
SKIP_FRONTEND=0
CLEAN=0
CHECK_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --release) MODE="release" ;;
        --skip-frontend) SKIP_FRONTEND=1 ;;
        --clean) CLEAN=1 ;;
        --check) CHECK_ONLY=1 ;;
        *) echo "未知参数: $arg（支持 --release/--skip-frontend/--clean/--check）"; exit 1 ;;
    esac
done

# ---------- 常量 ----------
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MOBILE_DIR="$REPO_ROOT/apps/mobile"
ANDROID_DIR="$MOBILE_DIR/android"
JNI_DIR="$ANDROID_DIR/app/src/main/jniLibs/arm64-v8a"
ASSETS_DIR="$ANDROID_DIR/app/src/main/assets"

# 环境探测（按优先级: env → 常规路径）
find_sdk() {
    if [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME/ndk" ]; then
        echo "$ANDROID_HOME"; return
    fi
    for d in "$HOME/Android/Sdk" "$HOME/android-sdk" "/opt/android-sdk"; do
        [ -d "$d/ndk" ] && { echo "$d"; return; }
    done
    echo ""
}
find_ndk() {
    local sdk="$1"
    [ -d "$sdk/ndk" ] || return 1
    ls -1 "$sdk/ndk" | sort -V | tail -1
}

SDK_ROOT="$(find_sdk)"
[ -z "$SDK_ROOT" ] && { echo "❌ Android SDK 未找到（设 ANDROID_HOME 或放 ~/Android/Sdk）"; exit 1; }
NDK_VERSION="$(find_ndk "$SDK_ROOT")"
[ -z "$NDK_VERSION" ] && { echo "❌ NDK 未安装: $SDK_ROOT/ndk"; exit 1; }
NDK_ROOT="$SDK_ROOT/ndk/$NDK_VERSION"
TOOLCHAIN="$NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64"
API_LEVEL=24
CLANG="$TOOLCHAIN/bin/aarch64-linux-android${API_LEVEL}-clang"

# Java（判定含 jlink — gradle JdkImageTransform 必需; 系统 openjdk 可能缺）
find_java() {
    for d in "${JAVA_HOME:-}" $HOME/jdk* /opt/jdk* /usr/lib/jvm/java-2*-openjdk-*; do
        [ -x "$d/bin/java" ] && [ -x "$d/bin/jlink" ] && { echo "$d"; return; }
    done
    # 兜底: PATH java（可能缺 jlink — 最后手段）
    command -v java >/dev/null 2>&1 && { dirname "$(dirname "$(command -v java)")"; return; }
    echo ""
}
JAVA_HOME_FOUND="$(find_java)"

# Gradle: 项目自带 wrapper 或全局
find_gradle() {
    [ -x "$ANDROID_DIR/gradlew" ] && { echo "$ANDROID_DIR/gradlew"; return; }
    command -v gradle >/dev/null 2>&1 && { echo "gradle"; return; }
    for d in /opt/gradle*/bin/gradle "$HOME"/gradle*/bin/gradle; do
        [ -x "$d" ] && { echo "$d"; return; }
    done
    echo ""
}
GRADLE_CMD="$(find_gradle)"

# ---------- 环境自检 ----------
# Rust env 提前（自检需要 cargo）
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export PATH="$CARGO_HOME/bin:$HOME/.local/rust/usr/bin:$PATH"

echo "════════════════════════════════════════════════"
echo " Aurora 一键构建（$MODE）"
echo "════════════════════════════════════════════════"
echo " SDK:      $SDK_ROOT"
echo " NDK:      $NDK_VERSION"
echo " clang:    $CLANG"
[ -n "$JAVA_HOME_FOUND" ] && echo " JAVA:     $JAVA_HOME_FOUND"
echo " Gradle:   $GRADLE_CMD"
echo "════════════════════════════════════════════════"

MISSING=0
[ -x "$CLANG" ] || { echo "❌ NDK clang 缺失"; MISSING=1; }
[ -n "$GRADLE_CMD" ] || { echo "❌ Gradle 未找到（装 gradle 或项目加 wrapper）"; MISSING=1; }
command -v node >/dev/null 2>&1 || { echo "❌ node 缺失（前端构建需要）"; MISSING=1; }
command -v cargo >/dev/null 2>&1 || { echo "❌ cargo 缺失"; MISSING=1; }
[ "$MISSING" = 1 ] && exit 1
echo "✅ 环境自检通过"

# 工具链: 固定 1.91（iroh 1.0+ 需要的版本; 可按 rust-toolchain.toml 演进）
RUST_TOOLCHAIN="1.91.0"
CARGO="cargo +$RUST_TOOLCHAIN"
if ! rustup toolchain list | grep -q "$RUST_TOOLCHAIN"; then
    echo "📦 安装 Rust $RUST_TOOLCHAIN..."
    rustup toolchain install "$RUST_TOOLCHAIN"
fi

# Rust target
if ! rustup target list --toolchain "$RUST_TOOLCHAIN" --installed | grep -q aarch64-linux-android; then
    echo "📦 安装 aarch64-linux-android target..."
    rustup target add aarch64-linux-android --toolchain "$RUST_TOOLCHAIN"
fi

[ "$CHECK_ONLY" = 1 ] && { echo "（--check 完成）"; exit 0; }

cd "$REPO_ROOT"

# ---------- 1. 前端 ----------
if [ "$SKIP_FRONTEND" = 0 ]; then
    echo "── [1/3] 前端构建（vite）──"
    (cd "$MOBILE_DIR" && npx vite build)
    mkdir -p "$ASSETS_DIR"
    cp "$MOBILE_DIR/dist/index.html" "$ASSETS_DIR/index.html"
else
    echo "── [1/3] 跳过前端（复用 dist/）──"
    [ -f "$MOBILE_DIR/dist/index.html" ] || { echo "❌ dist/index.html 不存在，去掉 --skip-frontend"; exit 1; }
fi

# ---------- 2. FFI 交叉编译 ----------
echo "── [2/3] Rust FFI 交叉编译（aarch64, $MODE）──"
if [ "$CLEAN" = 1 ]; then
    cargo clean -p aurora-mobile-ffi
fi
export CC_aarch64_linux_android="$CLANG"
export AR_aarch64_linux_android="$TOOLCHAIN/bin/llvm-ar"
export CARGO_INCREMENTAL=0
# native 恒 release（Android 惯例: debug 符号版 ~660MB 不可分发;
# APK debug/release 仅指 Java 层 + 打包配置）
$CARGO build -p aurora-mobile-ffi --features p2p-sync --release --target aarch64-linux-android

SO_SRC="$REPO_ROOT/target/aarch64-linux-android/release/libaurora_mobile_ffi.so"
[ -f "$SO_SRC" ] || { echo "❌ FFI 产物缺失: $SO_SRC"; exit 1; }
mkdir -p "$JNI_DIR"
cp "$SO_SRC" "$JNI_DIR/libaurora_mobile_ffi.so"
echo "   → jniLibs/arm64-v8a/libaurora_mobile_ffi.so ($(du -h "$SO_SRC" | cut -f1))"

# ---------- 3. APK ----------
echo "── [3/3] Gradle APK（$MODE）──"
GRADLE_TASK="assembleDebug"
[ "$MODE" = "release" ] && GRADLE_TASK="assembleRelease"
(
    cd "$ANDROID_DIR"
    export ANDROID_HOME="$SDK_ROOT"
    [ -n "$JAVA_HOME_FOUND" ] && export JAVA_HOME="$JAVA_HOME_FOUND"
    # local.properties 兜底（sdk.dir）
    grep -q "sdk.dir" local.properties 2>/dev/null || \
        echo "sdk.dir=$SDK_ROOT" >> local.properties
    "$GRADLE_CMD" "$GRADLE_TASK" --no-daemon
)

APK="$ANDROID_DIR/app/build/outputs/apk/$MODE/app-$MODE.apk"
echo "════════════════════════════════════════════════"
if [ -f "$APK" ]; then
    echo "✅ 构建成功: $APK ($(du -h "$APK" | cut -f1))"
    echo "   （Android 侧 release 需签名: apksigner / debug keystore）"
else
    echo "❌ APK 未生成（查上方 gradle 日志）"
    exit 1
fi
