#!/usr/bin/env bash
# ad-hoc 署名のサンドボックス debug ビルドで、フィクスチャ読取と無関係パス拒否を証明する。
# $TEAM_ID プレースホルダは署名前に必ず外す（残すとカーネルが SIGKILL する）。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
APP_DIR="$ROOT/apps/winzip-jp"
TAURI="$APP_DIR/src-tauri"
ARTIFACTS="${SANDBOX_ARTIFACTS:-$APP_DIR/e2e/artifacts}"
mkdir -p "$ARTIFACTS"

cd "$APP_DIR"
pnpm exec tauri build --debug \
  --config src-tauri/tauri.appstore.conf.json \
  --features e2e \
  --bundles app

APP="$ROOT/target/debug/bundle/macos/送りZIP.app"
if [[ ! -d "$APP" ]]; then
  APP="$TAURI/target/debug/bundle/macos/送りZIP.app"
fi
if [[ ! -d "$APP" ]]; then
  echo "サンドボックス .app が見つかりません: $ROOT/target/debug/bundle/macos/送りZIP.app" >&2
  exit 1
fi
echo "app=${APP}"

FIXTURE_SRC="$ROOT/tests/fixtures/read/cp932-typical.zip"
FIXTURE_DST="$APP/Contents/Resources/e2e-fixtures"
mkdir -p "$FIXTURE_DST"
cp "$FIXTURE_SRC" "$FIXTURE_DST/cp932-typical.zip"
FIXTURE="$FIXTURE_DST/cp932-typical.zip"

DENY="${SANDBOX_DENY_PATH:-$ARTIFACTS/e2e-deny-secret.txt}"
echo "deny-secret" > "$DENY"

ENTITLEMENTS="$TAURI/Entitlements.plist"
TMP_ENT="$(mktemp)"
# ad-hoc では $TEAM_ID プレースホルダを外す。権限セットは §4.1 のまま。
python3 - "$ENTITLEMENTS" "$TMP_ENT" <<'PY'
import plistlib
import sys
from pathlib import Path

src, dst = Path(sys.argv[1]), Path(sys.argv[2])
data = plistlib.loads(src.read_bytes())
for key in (
    "com.apple.application-identifier",
    "com.apple.developer.team-identifier",
):
    data.pop(key, None)
for key, value in list(data.items()):
    if isinstance(value, str) and "$TEAM_ID" in value:
        data.pop(key)
plistlib.dump(data, dst.open("wb"))
PY
# --deep は WKWebView ヘルパーにアプリ entitlements を押し付けるので使わない。
codesign --force --entitlements "$TMP_ENT" --sign - "$APP"
rm -f "$TMP_ENT"

codesign --verify --verbose=2 "$APP"
codesign -d --entitlements :- "$APP" > "$ARTIFACTS/signed-entitlements.xml"
if grep -qF '$TEAM_ID' "$ARTIFACTS/signed-entitlements.xml"; then
  echo "ad-hoc 署名に \$TEAM_ID が残っています" >&2
  exit 1
fi
if ! grep -q 'com.apple.security.app-sandbox' "$ARTIFACTS/signed-entitlements.xml"; then
  echo "app-sandbox が署名にありません" >&2
  exit 1
fi
if ! grep -q 'com.apple.security.network.client' "$ARTIFACTS/signed-entitlements.xml"; then
  echo "network.client が署名にありません（WKWebView が白画面になる）" >&2
  exit 1
fi

BIN="$APP/Contents/MacOS/winzip-jp"
if [[ ! -x "$BIN" ]]; then
  BIN="$APP/Contents/MacOS/送りZIP"
fi
if [[ ! -x "$BIN" ]]; then
  echo "実行ファイルがありません: $APP/Contents/MacOS" >&2
  ls -la "$APP/Contents/MacOS" >&2 || true
  exit 1
fi
if ! strings "$BIN" | grep -F -- "--e2e-open" >/dev/null; then
  echo "debug e2e ビルドに --e2e-open がありません" >&2
  exit 1
fi

REPORT="$ARTIFACTS/sandbox-probe.json"
SPLIT_OUT="$ARTIFACTS/e2e-split-outside/out.zip"
mkdir -p "$(dirname "$SPLIT_OUT")"
# JSON は stdout。サンドボックスはワークスペースへ書けない。判定は report_passes（終了コード）。
set +e
"$BIN" \
  --e2e-open "$FIXTURE" \
  --e2e-deny "$DENY" \
  --e2e-split-dest "$SPLIT_OUT" \
  > "$REPORT"
status=$?
set -e
if [[ ! -s "$REPORT" ]]; then
  echo "sandbox-probe.json が空です (exit=${status})" >&2
  exit 1
fi
python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1], encoding="utf-8")), ensure_ascii=False, indent=2))' "$REPORT"
if [[ "$status" -ne 0 ]]; then
  echo "sandbox probe 失敗: exit=${status}" >&2
  exit "$status"
fi
echo "sandbox probe ok"

# S1 相当: 起動直後の作成ドロップゾーン。フィクスチャは開かない。
open "$APP" || true
sleep 2
osascript -e 'tell application "送りZIP" to activate' >/dev/null 2>&1 || true
sleep 10
screencapture -x "$ARTIFACTS/s1-sandbox.png" || true
# スクリプト自身（パスに winzip-jp を含む）は殺さない
killall "送りZIP" >/dev/null 2>&1 || true
killall "winzip-jp" >/dev/null 2>&1 || true

if [[ ! -s "$ARTIFACTS/s1-sandbox.png" ]]; then
  echo "s1-sandbox.png が空です" >&2
  exit 1
fi
# ウィンドウ内（メニューバー・Dock を除く中央）がほぼ白なら WebView 未描画。
python3 - "$ARTIFACTS/s1-sandbox.png" <<'PY'
import struct, sys, zlib
from pathlib import Path

path = Path(sys.argv[1])
data = path.read_bytes()
if data[:8] != b"\x89PNG\r\n\x1a\n":
    raise SystemExit("s1-sandbox.png が PNG ではありません")
pos = 8
width = height = bit_depth = color_type = None
idat = bytearray()
while pos + 8 <= len(data):
    ln = struct.unpack(">I", data[pos : pos + 4])[0]
    typ = data[pos + 4 : pos + 8]
    chunk = data[pos + 8 : pos + 8 + ln]
    pos += 12 + ln
    if typ == b"IHDR":
        width, height, bit_depth, color_type = struct.unpack(">IIBB", chunk[:10])
    elif typ == b"IDAT":
        idat += chunk
    elif typ == b"IEND":
        break
if width is None or bit_depth != 8 or color_type not in (2, 6):
    raise SystemExit(f"未対応の PNG: {width}x{height} depth={bit_depth} type={color_type}")
raw = zlib.decompress(bytes(idat))
bpp = 4 if color_type == 6 else 3
stride = width * bpp

def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c

rows = []
i = 0
prev = bytearray(stride)
for _ in range(height):
    f = raw[i]
    i += 1
    row = bytearray(raw[i : i + stride])
    i += stride
    if f == 1:
        for x in range(stride):
            row[x] = (row[x] + (row[x - bpp] if x >= bpp else 0)) & 255
    elif f == 2:
        for x in range(stride):
            row[x] = (row[x] + prev[x]) & 255
    elif f == 3:
        for x in range(stride):
            a = row[x - bpp] if x >= bpp else 0
            row[x] = (row[x] + ((a + prev[x]) // 2)) & 255
    elif f == 4:
        for x in range(stride):
            a = row[x - bpp] if x >= bpp else 0
            c = prev[x - bpp] if x >= bpp else 0
            row[x] = (row[x] + paeth(a, prev[x], c)) & 255
    elif f != 0:
        raise SystemExit(f"png filter {f}")
    rows.append(row)
    prev = row

x0, x1 = int(width * 0.15), int(width * 0.85)
y0, y1 = int(height * 0.14), int(height * 0.72)
nonwhite = total = 0
for y in range(y0, y1):
    row = rows[y]
    for x in range(x0, x1):
        r, g, b = row[x * bpp], row[x * bpp + 1], row[x * bpp + 2]
        total += 1
        if r < 245 or g < 245 or b < 245:
            nonwhite += 1
ratio = nonwhite / total if total else 0
print(f"s1 nonwhite={nonwhite}/{total} ({ratio:.3f})")
if ratio < 0.02:
    raise SystemExit("s1-sandbox.png の WebView が白画面のままです")
PY
echo "wrote $ARTIFACTS/s1-sandbox.png"
