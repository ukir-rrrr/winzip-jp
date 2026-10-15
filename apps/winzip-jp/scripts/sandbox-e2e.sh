#!/usr/bin/env bash
# ad-hoc 署名のサンドボックス debug ビルドで、フィクスチャ読取と無関係パス拒否を証明する。
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

APP="$(find "$ROOT/target" "$TAURI/target" -path '*bundle/macos/*.app' -type d 2>/dev/null | head -1 || true)"
if [[ -z "${APP}" ]]; then
  APP="$(find "$ROOT" -name '*.app' -type d | head -1 || true)"
fi
if [[ -z "${APP}" ]]; then
  echo "サンドボックス .app が見つかりません" >&2
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
sign() {
  codesign --force --deep --entitlements "$1" --sign - "$APP"
}

if ! sign "$ENTITLEMENTS"; then
  TMP_ENT="$(mktemp)"
  # ad-hoc では $TEAM_ID プレースホルダを外す。権限セットは §4.1 のまま。
  python3 - "$ENTITLEMENTS" "$TMP_ENT" <<'PY'
import pathlib, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
for key in (
    "com.apple.application-identifier",
    "com.apple.developer.team-identifier",
):
    start = text.find(f"\t<key>{key}</key>")
    if start < 0:
        continue
    end = text.find("</string>", start)
    end = text.find("\n", end)
    text = text[:start] + text[end + 1 :]
dst.write_text(text)
PY
  sign "$TMP_ENT"
  rm -f "$TMP_ENT"
fi
codesign --verify --verbose=2 "$APP" || true
codesign -d --entitlements :- "$APP" | tee "$ARTIFACTS/signed-entitlements.xml" >/dev/null || true

BIN_DIR="$APP/Contents/MacOS"
BIN="$(find "$BIN_DIR" -type f -perm -111 -print -quit)"
if [[ -z "${BIN}" ]]; then
  echo "実行ファイルがありません: $BIN_DIR" >&2
  exit 1
fi
if ! strings "$BIN" | grep -F -- "--e2e-open" >/dev/null; then
  echo "debug e2e ビルドに --e2e-open がありません" >&2
  exit 1
fi

REPORT="$ARTIFACTS/sandbox-probe.json"
SPLIT_OUT="$ARTIFACTS/e2e-split-outside/out.zip"
mkdir -p "$(dirname "$SPLIT_OUT")"
"$BIN" \
  --e2e-open "$FIXTURE" \
  --e2e-deny "$DENY" \
  --e2e-split-dest "$SPLIT_OUT" \
  --e2e-report "$REPORT"

python3 - "$REPORT" <<'PY'
import json, sys
path = sys.argv[1]
report = json.loads(open(path, encoding="utf-8").read())
print(json.dumps(report, ensure_ascii=False, indent=2))
open_ok = report.get("open", {}).get("ok")
deny = report.get("deny", {})
deny_ok = deny.get("ok")
deny_err = deny.get("error") or ""
split = report.get("splitContainer") or {}
if not open_ok:
    raise SystemExit(f"フィクスチャを読めません: {report.get('open')}")
if deny_ok:
    raise SystemExit(f"無関係パスが読めてしまう: {deny}")
if "アクセスできません" not in deny_err:
    raise SystemExit(f"拒否がコマンド Err ではない: {deny_err!r}")
if not split.get("ok") or not split.get("z01"):
    raise SystemExit(f"コンテナ内の分割保存に失敗: {split}")
outside = report.get("splitOutside")
if outside is not None and outside.get("ok"):
    print("警告: サンドボックス外への分割保存が成功した（カーネル許可が広い）")
print("sandbox probe ok")
PY

# S1 相当: 起動直後の作成ドロップゾーン。フィクスチャは開かない。
open "$APP" || true
sleep 6
screencapture -x "$ARTIFACTS/s1-sandbox.png" || true
# スクリプト自身（パスに winzip-jp を含む）は殺さない
killall "送りZIP" >/dev/null 2>&1 || true
if [[ -n "${BIN}" ]]; then
  kill "$(pgrep -f "$BIN" || true)" >/dev/null 2>&1 || true
fi

if [[ ! -s "$ARTIFACTS/s1-sandbox.png" ]]; then
  echo "s1-sandbox.png が空です" >&2
  exit 1
fi
echo "wrote $ARTIFACTS/s1-sandbox.png"
