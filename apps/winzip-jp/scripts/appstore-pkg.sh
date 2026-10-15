#!/usr/bin/env bash
# App Store 向け .app から .pkg まで到達する。提出・アップロードはしない。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
APP_DIR="$ROOT/apps/winzip-jp"
TAURI="$APP_DIR/src-tauri"
ARTIFACTS="${APPSTORE_ARTIFACTS:-$APP_DIR/e2e/artifacts}"
mkdir -p "$ARTIFACTS"

cd "$APP_DIR"
# release。--features e2e は付けない。
pnpm exec tauri build \
  --config src-tauri/tauri.appstore.conf.json \
  --bundles app

APP="$ROOT/target/release/bundle/macos/送りZIP.app"
if [[ ! -d "$APP" ]]; then
  APP="$TAURI/target/release/bundle/macos/送りZIP.app"
fi
if [[ ! -d "$APP" ]]; then
  echo "App Store 用 .app が見つかりません: $ROOT/target/release/bundle/macos/送りZIP.app" >&2
  exit 1
fi
echo "app=${APP}"

BIN="$APP/Contents/MacOS/winzip-jp"
if [[ ! -x "$BIN" ]]; then
  BIN="$APP/Contents/MacOS/送りZIP"
fi
if [[ ! -x "$BIN" ]]; then
  echo "実行ファイルがありません: $APP/Contents/MacOS" >&2
  ls -la "$APP/Contents/MacOS" >&2 || true
  exit 1
fi
STRINGS="$(mktemp)"
strings "$BIN" > "$STRINGS"
if grep -Fq -- "--e2e-open" "$STRINGS"; then
  rm -f "$STRINGS"
  echo "App Store バイナリに --e2e-open がある" >&2
  exit 1
fi
rm -f "$STRINGS"

INFO="$APP/Contents/Info.plist"
if [[ ! -f "$INFO" ]]; then
  echo "Info.plist がありません: $INFO" >&2
  exit 1
fi
if ! grep -q ITSAppUsesNonExemptEncryption "$INFO"; then
  echo "Info.plist に ITSAppUsesNonExemptEncryption がありません" >&2
  exit 1
fi
if ! grep -q public.app-category.utilities "$INFO"; then
  echo "Info.plist のカテゴリが Utility ではありません" >&2
  exit 1
fi

PKG="$ARTIFACTS/winzip-jp.pkg"
# 提出キーがあっても upload しない（--no-upload）。
if ! command -v productbuild >/dev/null; then
  echo "productbuild がありません。pkg をスキップします" >&2
  exit 1
fi
productbuild --component "$APP" /Applications "$PKG"

if [[ ! -s "$PKG" ]]; then
  echo ".pkg が空です" >&2
  exit 1
fi
echo "pkg=$PKG ($(wc -c < "$PKG") bytes)"
# secrets があっても提出しない
echo "skip upload (--no-upload)"
