#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""create_issues.py — 解析 v26_issue清单.md 并调用 gh issue create / edit（幂等）。

用法:
    python3 create_issues.py                    # 提交全部卡
    python3 create_issues.py --filter R-00      # 只提交 R-00
    python3 create_issues.py --filter R-01 R-02 RV-01

前置:
    1) 安装 gh CLI 并完成认证: gh auth login
    2) 已执行清单第 0 节的标签与里程碑初始化脚本

幂等逻辑:
    以标题中的 [卡号] 为唯一键。先 gh issue list --search "[卡号] in:title"
    查重：存在则 gh issue edit（正文/标签/里程碑覆盖），不存在则 create。
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = "Lizeyun8501/Aurora"
LIST_FILE = Path(__file__).parent / "v26_issue清单.md"

CARD_HEAD = re.compile(r"^## \[([A-Z0-9]+-[0-9A-Z]+)\]\s*(.+)$", re.M)


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def gh(args, check=True):
    r = run(["gh"] + args)
    if check and r.returncode != 0:
        print(f"  gh 失败: {r.stderr.strip()[:300]}", file=sys.stderr)
        sys.exit(1)
    return r.stdout.strip()


def parse_cards():
    text = LIST_FILE.read_text(encoding="utf-8")
    heads = list(CARD_HEAD.finditer(text))
    cards = []
    for i, m in enumerate(heads):
        end = heads[i + 1].start() if i + 1 < len(heads) else len(text)
        body = text[m.start():end]
        cards.append({"id": m.group(1), "title": m.group(2).strip(), "body": body.rstrip() + "\n"})
    return cards


def existing_issue(card_id):
    out = gh(["issue", "list", "-R", REPO, "--state", "all",
              "--search", f"[{card_id}] in:title", "--json", "number,title", "--limit", "5"])
    items = json.loads(out) if out else []
    for it in items:
        if f"[{card_id}]" in it["title"]:
            return it["number"]
    return None


def milestone_number(title):
    if not title:
        return None
    out = gh(["api", f"repos/{REPO}/milestones", "--jq",
              f'.[] | select(.title == "{title}") | .number'])
    return out.strip() or None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--filter", nargs="*", help="只提交指定卡号（如 R-00 RV-01）")
    ap.add_argument("--dry-run", action="store_true", help="只解析不提交")
    a = ap.parse_args()

    cards = parse_cards()
    print(f"解析到 {len(cards)} 张卡: {', '.join(c['id'] for c in cards)}")
    if a.filter:
        want = {f.upper() for f in a.filter}
        cards = [c for c in cards if c["id"] in want]
        print(f"过滤后 {len(cards)} 张: {', '.join(c['id'] for c in cards)}")

    ms_cache = {}
    for c in cards:
        labels = re.search(r"\*\*Labels?\*\*:\s*(.+)", c["body"])
        labels = [x.strip().strip("`").strip() for x in re.split(r"[,，]", labels.group(1)) if x.strip().strip("`").strip()] if labels else []
        ms = re.search(r"\*\*Milestone\*\*:\s*`?([^`\n]+)`?", c["body"])
        ms_title = ms.group(1).strip() if ms else None

        if a.dry_run:
            print(f"  [DRY] {c['id']} {c['title'][:50]} labels={labels} milestone={ms_title}")
            continue

        num = existing_issue(c["id"])
        args_base = ["issue", "-R", REPO]
        if num:
            cmd = ["issue", "edit", str(num), "-R", REPO,
                   "--title", f"[{c['id']}] {c['title']}", "--body-file", "-"]
            for lb in labels:
                cmd += ["--add-label", lb]
            print(f"  {c['id']} -> 更新 #{num}")
        else:
            cmd = ["issue", "create", "-R", REPO,
                   "--title", f"[{c['id']}] {c['title']}", "--body-file", "-"]
            for lb in labels:
                cmd += ["--label", lb]
            print(f"  {c['id']} -> 新建")

        if ms_title:
            if ms_title not in ms_cache:
                ms_cache[ms_title] = milestone_number(ms_title)
            if ms_cache[ms_title]:
                cmd += ["--milestone", ms_cache[ms_title]]

        r = run(cmd, input=c["body"])
        if r.returncode != 0:
            print(f"  !! {c['id']} 失败: {r.stderr.strip()[:200]}", file=sys.stderr)
        else:
            print(f"  OK {r.stdout.strip().splitlines()[-1] if r.stdout.strip() else ''}")

    print("完成。")


if __name__ == "__main__":
    main()
