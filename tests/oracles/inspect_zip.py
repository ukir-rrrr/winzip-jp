#!/usr/bin/env python3
"""ZIP の生ファイル名バイトと gpbf を検査する。

Python の zipfile は bit 11 なしを CP437 としてデコードする。
CP437 は 0x00-0xFF 全単射なので encode('cp437') で生バイトが戻るが、
本スクリプトはヘッダを自前で読む（name_raw / gpbf の正）。

使い方:
  python tests/oracles/inspect_zip.py out.zip --expect tests/golden/foo/expected.json
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

SIG_LOCAL = 0x04034B50
SIG_CENTRAL = 0x02014B50
SIG_EOCD = 0x06054B50
EFS_UTF8_BIT = 0x0800
ENCRYPT_BIT = 0x0001
UNICODE_PATH_EXTRA_ID = 0x7075


def _u16(b: bytes, o: int) -> int:
    return struct.unpack_from("<H", b, o)[0]


def _u32(b: bytes, o: int) -> int:
    return struct.unpack_from("<I", b, o)[0]


def extra_has_id(extra: bytes, tag: int) -> bool:
    i = 0
    while i + 4 <= len(extra):
        eid = _u16(extra, i)
        size = _u16(extra, i + 2)
        i += 4
        if i + size > len(extra):
            return False
        if eid == tag:
            return True
        i += size
    return False


def find_eocd(data: bytes) -> int:
    if len(data) < 22:
        raise ValueError("file too small for EOCD")
    max_back = min(len(data) - 22, 65535)
    start = len(data) - 22 - max_back
    buf = data[start:]
    last = len(buf) - 22
    for i in range(last, -1, -1):
        if _u32(buf, i) != SIG_EOCD:
            continue
        comment_len = _u16(buf, i + 20)
        if i + 22 + comment_len == len(buf):
            return start + i
    raise ValueError("EOCD not found")


def inspect(data: bytes) -> list[dict]:
    eocd = find_eocd(data)
    cd_entries = _u16(data, eocd + 10)
    cd_size = _u32(data, eocd + 12)
    cd_offset = _u32(data, eocd + 16)
    cd = data[cd_offset : cd_offset + cd_size]
    entries = []
    i = 0
    for _ in range(cd_entries):
        if i + 46 > len(cd) or _u32(cd, i) != SIG_CENTRAL:
            raise ValueError("broken central directory")
        version_made_by = _u16(cd, i + 4)
        host_os = (version_made_by >> 8) & 0xFF
        gpbf = _u16(cd, i + 8)
        method = _u16(cd, i + 10)
        crc = _u32(cd, i + 16)
        name_len = _u16(cd, i + 28)
        extra_len = _u16(cd, i + 30)
        comment_len = _u16(cd, i + 32)
        local_offset = _u32(cd, i + 42)
        i += 46
        name_central = cd[i : i + name_len]
        i += name_len
        extra_central = cd[i : i + extra_len]
        i += extra_len + comment_len

        if _u32(data, local_offset) != SIG_LOCAL:
            raise ValueError("broken local header")
        local_gpbf = _u16(data, local_offset + 6)
        local_method = _u16(data, local_offset + 8)
        local_name_len = _u16(data, local_offset + 26)
        local_extra_len = _u16(data, local_offset + 28)
        name_off = local_offset + 30
        name_local = data[name_off : name_off + local_name_len]
        extra_local = data[name_off + local_name_len : name_off + local_name_len + local_extra_len]
        if name_local != name_central:
            raise ValueError("local/central name bytes differ")
        if local_gpbf != gpbf or local_method != method:
            raise ValueError("local/central gpbf or method differ")

        entries.append(
            {
                "name_hex": name_central.hex().upper(),
                "gpbf": gpbf,
                "method": method,
                "host_os": host_os,
                "crc": crc,
                "has_7075": extra_has_id(extra_local, UNICODE_PATH_EXTRA_ID)
                or extra_has_id(extra_central, UNICODE_PATH_EXTRA_ID),
                "extra_local_len": len(extra_local),
                "extra_central_len": len(extra_central),
            }
        )
    return entries


def norm_hex(s: str) -> str:
    return "".join(c for c in s.upper() if c in "0123456789ABCDEF")


def compare(entries: list[dict], expected: dict) -> list[str]:
    errors = []
    want = {
        (norm_hex(e["name_hex"]), int(e["gpbf"]), int(e["method"]))
        for e in expected["entries"]
    }
    got = {(e["name_hex"], e["gpbf"], e["method"]) for e in entries}
    if got != want:
        errors.append(f"entry set mismatch\n  got:  {sorted(got)}\n  want: {sorted(want)}")
    for e in entries:
        if e["gpbf"] & EFS_UTF8_BIT:
            errors.append(f"bit 11 set on {e['name_hex']}")
        if e["gpbf"] & ENCRYPT_BIT:
            errors.append(f"bit 0 set on {e['name_hex']}")
        if e["host_os"] != 0:
            errors.append(f"host OS {e['host_os']} != 0 on {e['name_hex']}")
        if e["has_7075"]:
            errors.append(f"0x7075 extra present on {e['name_hex']}")
        if e["extra_local_len"] or e["extra_central_len"]:
            errors.append(f"non-empty extra on {e['name_hex']}")
    for e in expected["entries"]:
        if int(e["gpbf"]) & EFS_UTF8_BIT:
            errors.append(f"expected.json gpbf has bit 11: {e['name_hex']}")
    excluded = expected.get("excluded") or []
    # 除外パスは ZIP に載らないことだけ見る（名前バイトの集合比較が本体）。
    _ = excluded
    return errors


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description="Inspect ZIP name_raw / gpbf")
    p.add_argument("zip_path")
    p.add_argument("--expect", help="tests/golden/*/expected.json")
    args = p.parse_args(argv)
    data = Path(args.zip_path).read_bytes()
    entries = inspect(data)
    if args.expect:
        expected = json.loads(Path(args.expect).read_text(encoding="utf-8"))
        errors = compare(entries, expected)
        if errors:
            print("inspect_zip.py FAILED", file=sys.stderr)
            for err in errors:
                print(err, file=sys.stderr)
            return 1
        print(f"ok: {len(entries)} entries match {args.expect}")
        return 0
    json.dump(entries, sys.stdout, ensure_ascii=False, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
