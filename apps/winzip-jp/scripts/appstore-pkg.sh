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

APP="$(find "$ROOT/target" "$TAURI/target" -path '*bundle/macos/*.app' -type d 2>/dev/null | grep -v /debug/ | head -1 || true)"
if [[ -z "${APP}" ]]; then
  APP="$(find "$ROOT/target/release" -name '*.app' -type d | head -1 || true)"
fi
if [[ -z "${APP}" ]]; then
  echo "App Store 用 .app が見つかりません" >&2
  exit 1
fi
echo "app=${APP}"

BIN="$(find "$APP/Contents/MacOS" -type f -perm -111 -print -quit)"
if [[ -n "${BIN}" ]] && strings "$BIN" | grep -F -- "--e2e-open" >/dev/null; then
  echo "App Store バイナリに --e2e-open がある" >&2
  exit 1
fi

INFO="$APP/Contents/Info.plist"
if [[ -f "$INFO" ]]; then
  if ! grep -q ITSAppUsesNonExemptEncryption "$INFO"; then
    echo "Info.plist に ITSAppUsesNonExemptEncryption がありません" >&2
    exit 1
  fi
  if ! grep -q public.app-category.utilities "$INFO"; then
    echo "Info.plist のカテゴリが Utility ではありません" >&2
    exit 1
  fi
fi

PKG="$ARTIFACTS/winzip-jp.pkg"
# 提出キーがあっても upload しない（--no-upload）。
if command -v productbuild >/dev/null; then
  productbuild --component "$APP" /Applications "$PKG"
else
  echo "productbuild がありません。pkg をスキップします" >&2
  exit 1
fi

if [[ ! -s "$PKG" ]]; then
  echo ".pkg が空です" >&2
  exit 1
fi
echo "pkg=$PKG ($(wc -c < "$PKG") bytes)"
# secrets があっても提出しない
echo "skip upload (--no-upload)"
