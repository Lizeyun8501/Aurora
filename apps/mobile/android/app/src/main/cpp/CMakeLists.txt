cmake_minimum_required(VERSION 3.18.1)
project(aurora_mobile_ffi)

# Rust static library path (pre-built by cargo)
# This will be set per-ABI by the build script
# The .so files are pre-built and placed in jniLibs/<abi>/

# We only need to compile the JNI bridge C++ and link against the pre-built .so
# Actually, the .so is already compiled, so we just need to make sure it's in jniLibs

# No CMake build needed — the Rust .so is pre-built and copied to jniLibs/
