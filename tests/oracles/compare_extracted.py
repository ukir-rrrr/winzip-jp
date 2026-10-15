#!/usr/bin/env python3
"""7-Zip -mcp=932 で展開した Unicode 名を expected.json の name_nfc と比較する。"""

from __future__ import annotations

import argparse
import json
import sys
import unicodedata
from pathlib import Path


def nfc(s: str) -> str:
    return unicodedata.normalize("NFC", s)


def collect(root: Path) -> set[str]:
    names: set[str] = set()
    if not root.exists():
        return names
    for path in root.rglob("*"):
        rel = path.relative_to(root).as_posix()
        if not rel or rel == ".":
            continue
        rel = nfc(rel)
        if path.is_dir():
            names.add(rel.rstrip("/") + "/")
        else:
            names.add(rel)
    return names


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser()
    p.add_argument("extracted_dir")
    p.add_argument("expect_json")
    args = p.parse_args(argv)
    expected = json.loads(Path(args.expect_json).read_text(encoding="utf-8"))
    want = {nfc(e["name_nfc"]) for e in expected["entries"] if e.get("name_nfc")}
    got = collect(Path(args.extracted_dir))
    if got != want:
        print("compare_extracted.py FAILED", file=sys.stderr)
        print("  got: ", sorted(got), file=sys.stderr)
        print("  want:", sorted(want), file=sys.stderr)
        return 1
    print(f"ok: extracted Unicode names match ({len(got)} entries)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
