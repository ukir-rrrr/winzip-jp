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
AES_EXTRA_ID = 0x9901


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
                "has_9901": extra_has_id(extra_local, AES_EXTRA_ID)
                and extra_has_id(extra_central, AES_EXTRA_ID),
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
    want_by_hex = {norm_hex(e["name_hex"]): e for e in expected["entries"]}
    for e in entries:
        if e["gpbf"] & EFS_UTF8_BIT:
            errors.append(f"bit 11 set on {e['name_hex']}")
        if e["host_os"] != 0:
            errors.append(f"host OS {e['host_os']} != 0 on {e['name_hex']}")
        if e["has_7075"]:
            errors.append(f"0x7075 extra present on {e['name_hex']}")
        expect_row = want_by_hex.get(e["name_hex"])
        expect_encrypt = bool(expect_row and int(expect_row["gpbf"]) & ENCRYPT_BIT)
        if expect_encrypt:
            if not (e["gpbf"] & ENCRYPT_BIT):
                errors.append(f"bit 0 missing on AES entry {e['name_hex']}")
            if not e["has_9901"]:
                errors.append(f"0x9901 extra missing on {e['name_hex']}")
        else:
            if e["gpbf"] & ENCRYPT_BIT:
                errors.append(f"bit 0 set on {e['name_hex']}")
            if e["extra_local_len"] or e["extra_central_len"]:
                errors.append(f"non-empty extra on {e['name_hex']}")
    for e in expected["entries"]:
        if int(e["gpbf"]) & EFS_UTF8_BIT:
            errors.append(f"expected.json gpbf has bit 11: {e['name_hex']}")
    excluded = expected.get("excluded") or []
    # 除外パスは ZIP に載らないことだけ見る（名前バイトの集合比較が本体）。
    _ = excluded
    return errors


def _disk_base(sizes: list[int], disk: int) -> int:
    if disk < 0 or disk >= len(sizes):
        raise ValueError(f"disk {disk} out of range ({len(sizes)} volumes)")
    return sum(sizes[:disk])


def linearize_chunks(chunks: list[bytes]) -> bytes:
    """マルチディスク分割なら CD/EOCD のオフセットを連結座標へ直す。

    7-Zip は EOCD ThisDisk != 0 のとき .z01 を探す。検査は連結ストリームで
    name_raw / gpbf を見るので、ディスク相対オフセットを足し直す。
    """
    if not chunks:
        return b""
    sizes = [len(c) for c in chunks]
    data = bytearray(b"".join(chunks))
    if len(chunks) == 1:
        return bytes(data)
    eocd = find_eocd(bytes(data))
    this_disk = _u16(data, eocd + 4)
    cd_disk = _u16(data, eocd + 6)
    if this_disk == 0 and cd_disk == 0:
        return bytes(data)
    cd_entries = _u16(data, eocd + 10)
    cd_size = _u32(data, eocd + 12)
    cd_rel = _u32(data, eocd + 16)
    cd_abs = _disk_base(sizes, cd_disk) + cd_rel
    i = cd_abs
    for _ in range(cd_entries):
        if i + 46 > len(data) or _u32(data, i) != SIG_CENTRAL:
            raise ValueError("broken central directory (split)")
        disk_start = _u16(data, i + 34)
        local_rel = _u32(data, i + 42)
        new_off = _disk_base(sizes, disk_start) + local_rel
        struct.pack_into("<H", data, i + 34, 0)
        struct.pack_into("<I", data, i + 42, new_off)
        name_len = _u16(data, i + 28)
        extra_len = _u16(data, i + 30)
        comment_len = _u16(data, i + 32)
        i += 46 + name_len + extra_len + comment_len
    if i != cd_abs + cd_size:
        raise ValueError("central directory size mismatch")
    struct.pack_into("<H", data, eocd + 4, 0)
    struct.pack_into("<H", data, eocd + 6, 0)
    struct.pack_into("<I", data, eocd + 16, cd_abs)
    return bytes(data)


def read_zip_bytes(path: Path) -> bytes:
    """WinZip 分割 (stem.z01 … stem.zip) なら連結し、必要ならオフセットを直す。"""
    first = path.with_name(path.stem + ".z01")
    if not first.is_file():
        return path.read_bytes()
    chunks: list[bytes] = []
    i = 1
    while True:
        piece = path.with_name(path.stem + f".z{i:02d}")
        if not piece.is_file():
            break
        chunks.append(piece.read_bytes())
        i += 1
        if i > 999:
            break
    chunks.append(path.read_bytes())
    return linearize_chunks(chunks)


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description="Inspect ZIP name_raw / gpbf")
    p.add_argument("zip_path")
    p.add_argument("--expect", help="tests/golden/*/expected.json")
    args = p.parse_args(argv)
    data = read_zip_bytes(Path(args.zip_path))
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
