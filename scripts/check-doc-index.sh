#!/usr/bin/env bash
# B-09: README「文档索引」断链自检——表内每个路径必须存在（CI 门禁）。
# 规则: 取 | `path` | 形态的首列;ADR 短名（docs/adr/ADR-001）按前缀 glob 解析。
# 用法: bash scripts/check-doc-index.sh  →  零断链 RC=0;断链清单 RC=1
set -u
cd "$(dirname "$0")/.."
README=README.md
[ -f "$README" ] || { echo "FAIL: README.md 缺失"; exit 1; }

# 提取「## 文档索引」表段内的 `path` 反引号项
sect=$(awk '/^## 文档索引/{f=1;next} /^## /{f=0} f' "$README")
[ -n "$sect" ] || { echo "FAIL: README 无「## 文档索引」段"; exit 1; }

# 仅取表格行首列（| `path` 形态）——行内描述列的反引号项不进索引面
# （首列锚定防行内多反引号对误配——B-09 首跑实证）
paths=$(printf '%s\n' "$sect" | grep '^|' | grep -oP '^\|\s*\`\K[^\`]+' || true)
[ -n "$paths" ] || { echo "FAIL: 索引表无路径项"; exit 1; }

fails=0
total=0
while IFS= read -r p; do
  total=$((total+1))
  if [ -e "$p" ]; then
    echo "ok   $p"
  elif ls "$p"* >/dev/null 2>&1; then
    echo "ok   $p （前缀解析: $(ls -d "$p"* | head -1)）"
  else
    echo "FAIL $p （断链）"
    fails=$((fails+1))
  fi
done <<EOF
$paths
EOF

echo "== 文档索引: $((total-fails))/$total 存在 =="
[ "$fails" -eq 0 ] || exit 1
exit 0
