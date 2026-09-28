#!/usr/bin/env bash
# DK-17 S1 灾难恢复演练包装：备份三测试（round trip / 篡改检测 / integrity）
# 用法：bash scripts/dk17_drill.sh   （输出归档到回执引用）
set -e
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env" 2>/dev/null || true
echo "== DK-17 S1 恢复演练 $(date -Is) =="
cargo test -p aurora-core backup:: -- --nocapture
cargo test -p aurora-bootstrap dk17_backup
echo "== 演练通过：备份→校验→恢复→round trip 闭环实证 =="
