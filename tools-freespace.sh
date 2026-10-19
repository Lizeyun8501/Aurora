#!/bin/bash
# 磁盘救援（2026-10-19 验收爆盘沉淀）：30G 盘全 workspace debug 链周期性 100%。
# 原版只删 >5M 非产物类——漏 deps 里 GB 级 test 链接二进制（无扩展名大文件）。
cd "$(dirname "$0")" || exit 1
# 1. deps 大二进制（test/e2e 链接产物——rlib/rmeta 保留重链成本低）
find target/debug/deps -maxdepth 1 -type f ! -name "*.*" -size +50M -delete 2>/dev/null
find target/debug/deps -type f -size +5M ! -name "*.rlib" ! -name "*.rmeta" ! -name "*.so" ! -name "*.d" -delete 2>/dev/null
find target/debug -maxdepth 1 -type f -executable -delete 2>/dev/null
# 2. 增量缓存（可再生——GB 级）
rm -rf target/debug/incremental 2>/dev/null
# 3. rustc 临时目录（链接崩溃残留）
rm -rf /tmp/rustc* 2>/dev/null
# 4. cargo registry src 解压缓存（cache .crate 保留可重解压）
rm -rf ~/.cargo/registry/src 2>/dev/null
df -h / | tail -1
