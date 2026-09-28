#!/bin/bash
find target/debug/deps -type f -size +5M ! -name "*.rlib" ! -name "*.rmeta" ! -name "*.so" ! -name "*.d" -delete 2>/dev/null
find target/debug -maxdepth 1 -type f -executable -delete 2>/dev/null
df -h / | tail -1
