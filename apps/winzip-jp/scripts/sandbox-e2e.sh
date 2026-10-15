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
codesign --force --deep --entitlements "$TMP_ENT" --sign - "$APP"
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
sleep 6
screencapture -x "$ARTIFACTS/s1-sandbox.png" || true
# スクリプト自身（パスに winzip-jp を含む）は殺さない
killall "送りZIP" >/dev/null 2>&1 || true
killall "winzip-jp" >/dev/null 2>&1 || true

if [[ ! -s "$ARTIFACTS/s1-sandbox.png" ]]; then
  echo "s1-sandbox.png が空です" >&2
  exit 1
fi
echo "wrote $ARTIFACTS/s1-sandbox.png"
