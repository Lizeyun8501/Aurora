#!/bin/bash
set -euo pipefail

# Cross-compile aurora-mobile-ffi for Android
# Usage: ./build-android-libs.sh

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SDK_ROOT="${ANDROID_HOME:-/home/z/android-sdk}"
NDK_VERSION="26.3.11579264"
NDK_ROOT="$SDK_ROOT/ndk/$NDK_VERSION"
TOOLCHAIN="$NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64"
API_LEVEL=24

export PATH="/home/z/.local/rust/usr/bin:$PATH"
export LD_LIBRARY_PATH="/home/z/.local/rust/usr/lib/x86_64-linux-gnu:/home/z/.local/rust/usr/lib:$LD_LIBRARY_PATH"

cd "$REPO_ROOT"

# ABI configurations: <rust_target> <android_abi> <clang_prefix>
declare -a ABIS=(
    "aarch64-linux-android arm64-v8a aarch64-linux-android${API_LEVEL}"
    "armv7-linux-androideabi armeabi-v7a armv7a-linux-androideabi${API_LEVEL}"
    "x86_64-linux-android x86_64 x86_64-linux-android${API_LEVEL}"
)

JNI_DIR="$REPO_ROOT/apps/mobile/android/app/src/main/jniLibs"

for abi_entry in "${ABIS[@]}"; do
    read -r rust_target android_abi clang_prefix <<< "$abi_entry"
    echo "=== Building for $android_abi ($rust_target) ==="

    export CARGO_TARGET_${rust_target//-/_}_UPPER_UNDER=$(echo "$rust_target" | tr 'a-z-' 'A-Z_')
    # Simpler: use rustflags via .cargo/config
    export CC_${rust_target//-/_}="$TOOLCHAIN/bin/${clang_prefix}-clang"
    export CXX_${rust_target//-/_}="$TOOLCHAIN/bin/${clang_prefix}-clang++"
    export AR_${rust_target//-/_}="$TOOLCHAIN/bin/llvm-ar"
    export RANLIB_${rust_target//-/_}="$TOOLCHAIN/bin/llvm-ranlib"
    export LD_${rust_target//-/_}="$TOOLCHAIN/bin/ld"

    # Also set generic CC for the target
    export TARGET_CC="$TOOLCHAIN/bin/${clang_prefix}-clang"
    export TARGET_AR="$TOOLCHAIN/bin/llvm-ar"

    # Build
    cargo build -p aurora-mobile-ffi --release --target "$rust_target"

    # Copy .so to jniLibs
    mkdir -p "$JNI_DIR/$android_abi"
    cp "$REPO_ROOT/target/$rust_target/release/libaurora_mobile_ffi.so" "$JNI_DIR/$android_abi/"
    echo "  -> $JNI_DIR/$android_abi/libaurora_mobile_ffi.so"
done

echo "=== All ABIs built successfully ==="
