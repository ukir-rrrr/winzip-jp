#!/usr/bin/env python3
"""解凍テスト用の構造サンプル ZIP を決定的に作る（Stored、DOS 時刻固定）。"""

from __future__ import annotations

import argparse
import binascii
import json
import struct
from pathlib import Path

SIG_LOCAL = 0x04034B50
SIG_CENTRAL = 0x02014B50
SIG_EOCD = 0x06054B50
EFS = 0x0800
DOS_TIME = 0
DOS_DATE = (1 << 5) | 1  # 1980-01-01


def crc32(data: bytes) -> int:
    return binascii.crc32(data) & 0xFFFFFFFF


def put_entry(name: bytes, data: bytes, gpbf: int) -> tuple[bytes, bytes]:
    crc = crc32(data)
    size = len(data)
    local = struct.pack(
        "<IHHHHHIIIHH",
        SIG_LOCAL,
        20,
        gpbf,
        0,
        DOS_TIME,
        DOS_DATE,
        crc,
        size,
        size,
        len(name),
        0,
    )
    local += name + data
    central = struct.pack(
        "<IHHHHHHIIIHHHHHII",
        SIG_CENTRAL,
        20,
        20,
        gpbf,
        0,
        DOS_TIME,
        DOS_DATE,
        crc,
        size,
        size,
        len(name),
        0,
        0,
        0,
        0,
        0x20,
        0,  # local offset patched later
    )
    central += name
    return local, central


def write_zip(path: Path, entries: list[tuple[bytes, bytes, int]]) -> None:
    locals_blob = bytearray()
    centrals = bytearray()
    for name, data, gpbf in entries:
        local, central = put_entry(name, data, gpbf)
        offset = len(locals_blob)
        locals_blob += local
        # patch local offset (last 4 bytes before name)
        central = bytearray(central)
        struct.pack_into("<I", central, 42, offset)
        centrals += central
    cd_offset = len(locals_blob)
    cd_size = len(centrals)
    eocd = struct.pack(
        "<IHHHHIIH",
        SIG_EOCD,
        0,
        0,
        len(entries),
        len(entries),
        cd_size,
        cd_offset,
        0,
    )
    path.write_bytes(bytes(locals_blob) + bytes(centrals) + eocd)


def hex_of(b: bytes) -> str:
    return b.hex().upper()


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "fixtures" / "read",
    )
    args = p.parse_args()
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)

    ga_utf8 = "が.txt".encode("utf-8")
    docs_utf8 = "ドキュメント.txt".encode("utf-8")
    ga_cp932 = "が.txt".encode("cp932")
    docs_cp932 = "ドキュメント.txt".encode("cp932")
    ascii_name = b"readme.txt"

    cases = {
        "cp932-typical": {
            "encoding": "cp932",
            "confidence": 0.8,
            "ambiguous": False,
            "entries": [
                (ga_cp932, b"ga\n", 0),
                (docs_cp932, b"docs\n", 0),
                (ascii_name, b"ascii\n", 0),
            ],
        },
        "utf8-efs": {
            "encoding": "utf8",
            "confidence": 0.95,
            "ambiguous": False,
            "entries": [
                (ga_utf8, b"ga\n", EFS),
                (docs_utf8, b"docs\n", EFS),
                (ascii_name, b"ascii\n", EFS),
            ],
        },
        "utf8-finder": {
            "encoding": "utf8",
            "confidence": 0.8,
            "ambiguous": False,
            "entries": [
                (ga_utf8, b"ga\n", 0),
                (docs_utf8, b"docs\n", 0),
            ],
        },
        "ambiguous-mixed": {
            "encoding": "cp932",
            "confidence": 0.4,
            "ambiguous": True,
            "entries": [
                (ga_utf8, b"utf8-ga\n", 0),
                (docs_cp932, b"cp932-docs\n", 0),
            ],
        },
    }

    for name, spec in cases.items():
        zip_path = out / f"{name}.zip"
        write_zip(zip_path, spec["entries"])
        expected = {
            "encoding": spec["encoding"],
            "confidence": spec["confidence"],
            "ambiguous": spec["ambiguous"],
            "entries": [
                {
                    "name_hex": hex_of(n),
                    "gpbf": gpbf,
                    "size": len(data),
                }
                for n, data, gpbf in spec["entries"]
            ],
        }
        (out / f"{name}.json").write_text(
            json.dumps(expected, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        print(f"wrote {zip_path}")


if __name__ == "__main__":
    main()
