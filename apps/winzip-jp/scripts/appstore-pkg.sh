#!/usr/bin/env bash
# App Store 向け .app から .pkg まで到達する。提出・アップロードはしない。
# Team ID CHSRTG92SZ の Apple Distribution / Mac Installer Distribution が
# キーチェーンにあるときだけ提出用に署名する。無いときは CI 向けに ad-hoc。
# 証明書・秘密鍵・プロファイル・パスワード・API キーはリポジトリに置かない。
set -euo pipefail

TEAM_ID="CHSRTG92SZ"
BUNDLE_ID="jp.riku.winzip"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
APP_DIR="$ROOT/apps/winzip-jp"
TAURI="$APP_DIR/src-tauri"
APP_NAME="文字化けしないZIP"
ARTIFACTS="${APPSTORE_ARTIFACTS:-$APP_DIR/e2e/artifacts}"
mkdir -p "$ARTIFACTS"

# security find-identity の引用符付き名前から、種類と Team ID が両方ある最初の 1 件。
# `| head` は使わない（別 .app を拾う事故と同じクラスをテストが禁止している）。
find_identity() {
  local needle="$1"
  local listing=""
  listing="$(security find-identity -v 2>/dev/null || true)"
  printf '%s\n' "$listing" | awk -v team="$TEAM_ID" -v needle="$needle" '
    index($0, needle) && index($0, team) {
      if (match($0, /"[^"]+"/)) {
        print substr($0, RSTART + 1, RLENGTH - 2)
        exit
      }
    }
  '
}

DIST_IDENTITY="$(find_identity "Apple Distribution")"
INSTALLER_IDENTITY="$(find_identity "Mac Installer Distribution")"
if [[ -z "$INSTALLER_IDENTITY" ]]; then
  INSTALLER_IDENTITY="$(find_identity "3rd Party Mac Developer Installer")"
fi

PROFILE="${APPSTORE_PROVISION_PROFILE:-$TAURI/embedded.provisionprofile}"
NEED_PROFILE=0
ADHOC_CFG=""
EXPAND_DIR=""
TAURI_CONFIGS=(--config src-tauri/tauri.appstore.conf.json)

if [[ -n "$DIST_IDENTITY" ]]; then
  export APPLE_SIGNING_IDENTITY="$DIST_IDENTITY"
  echo "signingIdentity=${DIST_IDENTITY}"
else
  ADHOC_CFG="$(mktemp)"
  printf '%s\n' '{"bundle":{"macOS":{"signingIdentity":"-"}}}' > "$ADHOC_CFG"
  TAURI_CONFIGS+=(--config "$ADHOC_CFG")
  echo "Apple Distribution (${TEAM_ID}) が無いので ad-hoc で .pkg まで作る（CI / 提出不可）"
fi

cleanup() {
  if [[ -n "$ADHOC_CFG" ]]; then
    rm -f "$ADHOC_CFG"
  fi
  if [[ -n "$EXPAND_DIR" ]]; then
    rm -rf "$EXPAND_DIR"
  fi
}
trap cleanup EXIT

cd "$APP_DIR"
# release。--features e2e は付けない。
pnpm exec tauri build \
  "${TAURI_CONFIGS[@]}" \
  --bundles app

APP="$ROOT/target/release/bundle/macos/${APP_NAME}.app"
if [[ ! -d "$APP" ]]; then
  APP="$TAURI/target/release/bundle/macos/${APP_NAME}.app"
fi
if [[ ! -d "$APP" ]]; then
  echo "App Store 用 .app が見つかりません: $ROOT/target/release/bundle/macos/${APP_NAME}.app" >&2
  exit 1
fi
echo "app=${APP}"

BIN="$APP/Contents/MacOS/winzip-jp"
if [[ ! -x "$BIN" ]]; then
  BIN="$APP/Contents/MacOS/${APP_NAME}"
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
if grep -q ITSAppUsesNonExemptEncryption "$INFO"; then
  echo "Info.plist に ITSAppUsesNonExemptEncryption があります。輸出コンプライアンスは App Store Connect で答える" >&2
  exit 1
fi
if ! grep -q public.app-category.utilities "$INFO"; then
  echo "Info.plist のカテゴリが Utility ではありません" >&2
  exit 1
fi
if ! grep -A1 '<key>CFBundleDisplayName</key>' "$INFO" | grep -Fq "$APP_NAME"; then
  echo "Info.plist の CFBundleDisplayName が ${APP_NAME} ではありません" >&2
  exit 1
fi
if ! grep -q "$BUNDLE_ID" "$INFO"; then
  echo "Info.plist の Bundle ID が ${BUNDLE_ID} ではありません" >&2
  exit 1
fi

NEED_PROFILE=0
if [[ -n "$DIST_IDENTITY" ]]; then
  NEED_PROFILE=1
elif [[ -n "${APPSTORE_PROVISION_PROFILE:-}" ]]; then
  # CI が隔離属性付きダミーを渡し、コピー→剥がしを検証する。提出用署名ではない。
  NEED_PROFILE=1
fi
if [[ "$NEED_PROFILE" -eq 1 ]]; then
  if [[ ! -f "$PROFILE" ]]; then
    echo "提出用署名には Mac App Store のプロビジョニングプロファイルが必要です。" >&2
    echo "APPSTORE_PROVISION_PROFILE を渡すか、${TAURI}/embedded.provisionprofile に置く（gitignore 済み）。" >&2
    exit 1
  fi
  cp "$PROFILE" "$APP/Contents/embedded.provisionprofile"
fi
# Transporter 91109: ブラウザで落としたプロファイルの com.apple.quarantine が
# codesign にシールされると .pkg が拒否される。cp は macOS で xattr を引き継ぐ。
# コピー後・署名前に .app 全体から剥がす。
xattr -cr "$APP"
if [[ "$NEED_PROFILE" -eq 1 ]] \
  && xattr -p com.apple.quarantine "$APP/Contents/embedded.provisionprofile" >/dev/null 2>&1; then
  echo "embedded.provisionprofile に com.apple.quarantine が残っています（Transporter エラー 91109）" >&2
  xattr -l "$APP/Contents/embedded.provisionprofile" >&2 || true
  exit 1
fi

if [[ -n "$DIST_IDENTITY" ]]; then
  # deep 署名は WKWebView ヘルパーにアプリ entitlements を押し付けるので使わない。
  codesign --force --options runtime \
    --entitlements "$TAURI/Entitlements.plist" \
    --sign "$DIST_IDENTITY" \
    "$APP"
  codesign --verify --verbose=2 "$APP"
  SIGNED_ENT="$(mktemp)"
  codesign -d --entitlements :- "$APP" > "$SIGNED_ENT"
  if grep -qF '$TEAM_ID' "$SIGNED_ENT"; then
    rm -f "$SIGNED_ENT"
    echo "提出用署名に \$TEAM_ID プレースホルダが残っています" >&2
    exit 1
  fi
  if ! grep -q "${TEAM_ID}.${BUNDLE_ID}" "$SIGNED_ENT"; then
    rm -f "$SIGNED_ENT"
    echo "提出用署名に ${TEAM_ID}.${BUNDLE_ID} がありません" >&2
    exit 1
  fi
  rm -f "$SIGNED_ENT"
fi

PKG="$ARTIFACTS/winzip-jp.pkg"
# 提出キーがあっても upload しない（--no-upload）。
if ! command -v productbuild >/dev/null; then
  echo "productbuild がありません。pkg をスキップします" >&2
  exit 1
fi
if [[ -n "$INSTALLER_IDENTITY" ]]; then
  echo "installerIdentity=${INSTALLER_IDENTITY}"
  productbuild --sign "$INSTALLER_IDENTITY" --component "$APP" /Applications "$PKG"
elif [[ -n "$DIST_IDENTITY" ]]; then
  echo "Mac Installer Distribution（または 3rd Party Mac Developer Installer）が Team ${TEAM_ID} にありません" >&2
  exit 1
else
  productbuild --component "$APP" /Applications "$PKG"
fi

if [[ ! -s "$PKG" ]]; then
  echo ".pkg が空です" >&2
  exit 1
fi
echo "pkg=$PKG ($(wc -c < "$PKG") bytes)"
# productbuild は xattr をペイロードに残す。展開して quarantine が無いことを落とす。
EXPAND_DIR="${ARTIFACTS}/pkg-expand"
rm -rf "$EXPAND_DIR"
pkgutil --expand-full "$PKG" "$EXPAND_DIR"
QUARANTINE_LIST="$(xattr -lr "$EXPAND_DIR")"
QUARANTINE_HITS="$(printf '%s\n' "$QUARANTINE_LIST" | grep -F 'com.apple.quarantine' || true)"
if [[ -n "$QUARANTINE_HITS" ]]; then
  echo "pkg に com.apple.quarantine が残っています（Transporter エラー 91109）:" >&2
  printf '%s\n' "$QUARANTINE_HITS" >&2
  exit 1
fi
if [[ "$NEED_PROFILE" -eq 1 ]]; then
  EMBEDDED_PROFILE=""
  while IFS= read -r EMBEDDED_CANDIDATE; do
    EMBEDDED_PROFILE="$EMBEDDED_CANDIDATE"
    break
  done < <(find "$EXPAND_DIR" -name 'embedded.provisionprofile')
  if [[ -z "$EMBEDDED_PROFILE" ]]; then
    echo "埋め込みプロファイルが pkg にありません" >&2
    exit 1
  fi
fi
rm -rf "$EXPAND_DIR"
EXPAND_DIR=""
# secrets があっても提出しない
echo "skip upload (--no-upload)"
