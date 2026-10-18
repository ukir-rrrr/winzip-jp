# Mac App Store 提出手順（riku の Mac）

この文書は **riku 本人が自分の arm64 Mac** で、個人の Apple Developer チーム **CHSRTG92SZ** から「文字化けしないZIP」を提出するための手順である。エージェントや CI は提出しない。

Bundle ID は **`jp.riku.winzip`** のまま。表示名は **文字化けしないZIP**。価格は買い切り **¥900**。販売は **日本のみ**。

証明書・秘密鍵・プロビジョニングプロファイル・パスワード・API キーは **リポジトリに置かない**。`.gitignore` は `*.p12` `*.p8` `*.cer` `*.provisionprofile` `embedded.provisionprofile` `.env` を除外済み。

---

## 0. 前提

同じ Apple ID で次が終わっていること。

- Apple Developer Program（個人）に加入済み。Team ID が `CHSRTG92SZ`
- Xcode が入っている（Command Line Tools 含む）
- [Transporter](https://apps.apple.com/app/transporter/id1450873914) を Mac App Store から入れてある
- App Store Connect の **有料アプリ契約**（Agreements, Tax, and Banking）に同意し、税務と口座を入れてある
- このリポジトリをクローンしてある

確認:

```bash
uname -m
# arm64 であること。Intel / ユニバーサルバイナリは作らない。

xcode-select -p
xcodebuild -version
# Xcode のパスとバージョンが出ること。

security find-identity -v -p codesigning
git -C "$HOME/src/winzip-jp" rev-parse --show-toplevel 2>/dev/null || pwd
```

リポジトリの場所は自分のクローン先に読み替える。以下ではそのルートを `$REPO` と書く。

```bash
cd "$REPO"
git status
# 提出用の署名ファイルが staged に乗っていないこと。
```

---

## 1. App ID `jp.riku.winzip` を作る

ブラウザで [Identifiers](https://developer.apple.com/account/resources/identifiers/list) を開く。右上のチームが **CHSRTG92SZ** であること。

1. **+**
2. **App IDs** → Continue
3. 種類は **App** → Continue
4. 次を入れる。
   - Description: `文字化けしないZIP`
   - Bundle ID: **Explicit** → `jp.riku.winzip`
   - Platform: **macOS**（チェックが分かれている画面なら macOS だけ）
5. Capabilities は **App Sandbox** を付ける。その他（iCloud、Push、Associated Domains など）は付けない。
6. Register

できたら Identifier の詳細で、Prefix が `CHSRTG92SZ`、Bundle ID が `jp.riku.winzip` であることを見る。アプリケーション識別子は `CHSRTG92SZ.jp.riku.winzip`。これは `apps/winzip-jp/src-tauri/Entitlements.plist` と同じ値である。

---

## 2. 証明書を Xcode で作る

提出に必要なのは次の 2 枚。どちらもチーム **CHSRTG92SZ**。

| 種類 | 用途 |
| --- | --- |
| **Apple Distribution** | `.app` のコード署名 |
| **Mac Installer Distribution** | `.pkg` のインストーラ署名（古い名前は 3rd Party Mac Developer Installer） |

Developer ID Application / Developer ID Installer は **使わない**（ストア外配布用）。

### 2.1 Xcode にアカウントを入れる

1. Xcode を開く
2. **Xcode → Settings… → Accounts**（古い Xcode は Preferences）
3. 左下 **+** → **Apple ID**。Developer の Apple ID で入る
4. 右側の Team に個人チームが出て、Team ID が `CHSRTG92SZ` であること

### 2.2 Apple Distribution

1. その Team を選んで **Manage Certificates…**
2. 左下 **+** → **Apple Distribution**
3. エラーなく追加されること

### 2.3 Mac Installer Distribution

同じ **+** に **Mac Installer Distribution** があれば、それを選ぶ。

Xcode の一覧に無いときはポータルで作る。

1. [Certificates](https://developer.apple.com/account/resources/certificates/list) を開く
2. **+** → **Mac Installer Distribution** → Continue
3. CSR が要る。キーチェーンアクセスで作る。
   - キーチェーンアクセスを開く
   - **キーチェーンアクセス → 証明書アシスタント → 認証局に証明書を要求…**
   - ユーザのメールアドレス: 自分の Apple ID
   - 通称: `riku Mac Installer Distribution`
   - 「ディスクに保存」
   - 作った `.certSigningRequest` をポータルにアップロード
4. できた証明書を Download。`.cer` をダブルクリックしてログインキーチェーンへ入れる
5. `.cer` と `.certSigningRequest` はリポジトリに入れない。Downloads に残すか捨てる

確認（氏名はキーチェーンの表示名。Team ID が付いていればよい）:

```bash
security find-identity -v -p codesigning
# "Apple Distribution: <氏名> (CHSRTG92SZ)" が Valid であること。

security find-identity -v
# "Mac Installer Distribution: <氏名> (CHSRTG92SZ)"
# または "3rd Party Mac Developer Installer: <氏名> (CHSRTG92SZ)"
```

`CSSMERR_TP_CERT_REVOKED` や「valid identities が 0」なら、証明書を消し、2.2 / 2.3 からやり直す。秘密鍵はログインキーチェーンの「自分の証明書」に対になっていること。

---

## 3. Mac App Store のプロビジョニングプロファイル

1. [Profiles](https://developer.apple.com/account/resources/profiles/list) を開く
2. **+**
3. **Mac App Store Connect**（表示が古いときは **Mac App Store**）→ Continue
4. App ID: **jp.riku.winzip** → Continue
5. 証明書: さっきの **Apple Distribution**（CHSRTG92SZ）→ Continue
6. Profile Name: `winzip-jp Mac App Store` → Generate
7. **Download**

保存先は次のどちらでもよい。**git add しない。**

```bash
# 推奨: 提出のときだけ環境変数で渡す
mkdir -p "$HOME/Library/MobileDevice/Provisioning Profiles"
# ダウンロードしたファイル名はポータルが出す UUID になる。
ls "$HOME/Downloads"/*.provisionprofile

# リポジトリに置くなら gitignore 済みのこの名前だけ
# cp "$HOME/Downloads/<ファイル>.provisionprofile" \
#   "$REPO/apps/winzip-jp/src-tauri/embedded.provisionprofile"
```

Xcode が拾う場所（新しい Xcode）:

```bash
mkdir -p "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
cp "$HOME/Downloads/"*.provisionprofile \
  "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles/"
```

---

## 4. App Store Connect にアプリ記録を作る

[Apps](https://appstoreconnect.apple.com/apps) を開く。

1. **+** → **New App**
2. 次を入れる。あとから変えられないものがある。
   - Platforms: **macOS**
   - Name: **文字化けしないZIP**
   - Primary Language: **Japanese**（日本語）
   - Bundle ID: **jp.riku.winzip**（1. で作ったものが一覧に出る）
   - SKU: **jp.riku.winzip**（ストアには出ない。一度決めたら変えられない）
   - User Access: Full Access
3. Create

バージョンはリポジトリの `apps/winzip-jp/src-tauri/tauri.conf.json`・`apps/winzip-jp/package.json`・`apps/winzip-jp/src-tauri/Cargo.toml` が `1.0.0`。App Store Connect のバージョンは **1.0**。ビルド後の `CFBundleShortVersionString` / `CFBundleVersion` も `1.0.0` になる（Connect の 1.0 と対応する）。

---

## 5. 価格 ¥900 と販売国（日本のみ）

App Store Connect のそのアプリで **価格および入手可能性**（Monetization → Pricing and Availability）を開く。

1. 価格表で日本の価格を **¥900** にする。買い切りの有料アプリ。非消費型 IAP にはしない（`docs/store/listing.md`）。
2. 入手可能性（App Availability）で **日本だけ** を残す。他の国は外す。
3. **EU は入れない。** 個人アカウントで DSA のトレーダーではない。EU で売るにはトレーダー情報（事業者名・住所など）が要る。非トレーダーのまま EU を開けると提出できないか、ストアから外される。
   - Digital Services Act / トレーダーの質問が出たら **非トレーダー（I’m not a trader）**
   - EU 加盟国をまとめて外す UI があれば、それを使う
4. 保存

---

## 6. 署名済み `.pkg` をリポジトリのスクリプトで作る

クローン先で、3. のプロファイルを渡して既存スクリプトを走らせる。アップロードはスクリプトがしない。

```bash
cd "$REPO"

# プロファイルのパスは Downloads の実ファイル名に合わせる
export APPSTORE_PROVISION_PROFILE="$HOME/Downloads/winzip-jp_Mac_App_Store.provisionprofile"

# キーチェーンの署名を確認してからビルド
security find-identity -v -p codesigning | grep -F 'Apple Distribution'
security find-identity -v | grep -E 'Mac Installer Distribution|3rd Party Mac Developer Installer'

bash apps/winzip-jp/scripts/appstore-pkg.sh
```

`pnpm --filter winzip-jp build:appstore` も同じスクリプトを呼ぶ。

成功時の標準出力の目安:

```
signingIdentity=Apple Distribution: <氏名> (CHSRTG92SZ)
app=.../文字化けしないZIP.app
installerIdentity=Mac Installer Distribution: <氏名> (CHSRTG92SZ)
pkg=.../apps/winzip-jp/e2e/artifacts/winzip-jp.pkg
skip upload (--no-upload)
```

成果物は `apps/winzip-jp/e2e/artifacts/winzip-jp.pkg`。このディレクトリは gitignore 済み。

Apple Distribution が無いとスクリプトは CI と同じ ad-hoc `.pkg` を作り、提出には使えない。プロファイルが無いのに Distribution 証明書だけあるときは、スクリプトは失敗する。

手元で中身を見る:

```bash
APP="$REPO/target/release/bundle/macos/文字化けしないZIP.app"
if [[ ! -d "$APP" ]]; then
  APP="$REPO/apps/winzip-jp/src-tauri/target/release/bundle/macos/文字化けしないZIP.app"
fi

codesign --verify --verbose=2 "$APP"
codesign -d --entitlements :- "$APP"
# application-identifier が CHSRTG92SZ.jp.riku.winzip
# team-identifier が CHSRTG92SZ
# $TEAM_ID という文字が無いこと

pkgutil --check-signature "$REPO/apps/winzip-jp/e2e/artifacts/winzip-jp.pkg"
ls -lh "$APP/Contents/embedded.provisionprofile"
```

---

## 7. Transporter で上げる

スクリプトでは上げない。**Transporter** で上げる。

### 7.1 アプリ（推奨）

```bash
open -a Transporter
open -R "$REPO/apps/winzip-jp/e2e/artifacts/winzip-jp.pkg"
```

1. Transporter に Apple ID（チーム CHSRTG92SZ）で入る
2. `winzip-jp.pkg` をウィンドウへドロップ
3. 検証が通ったら **配信**（Deliver）
4. App Store Connect のそのバージョンにビルドが付くまで待つ（数分〜数十分）

### 7.2 コマンドライン

パスワードをコマンドに書かない。聞かれるか、キーチェーンのアプリ用パスワードを使う。

```bash
xcrun iTMSTransporter -m upload \
  -assetFile "$REPO/apps/winzip-jp/e2e/artifacts/winzip-jp.pkg" \
  -u "$APPLE_ID" \
  -asc_provider CHSRTG92SZ
```

`$APPLE_ID` は自分の Apple ID。`-p` を付けなければパスワードを聞かれる。アプリ用パスワードをキーチェーンに入れるなら、自分のマシンだけで行い、リポジトリにも issue にも書かない。

API キー（`.p8`）でも上げられるが、このガイドでは使わない。作った場合もコミットしない。

---

## 8. 掲載文を `docs/store/listing.md` から入れる

App Store Connect の macOS バージョン（日本語）を開き、`docs/store/listing.md` のコメントで囲んだ本文を **そのまま** 入れる。英語の併記はしない。絵文字は使わない。

| Connect の欄 | listing.md の印 | 値の要約 |
| --- | --- | --- |
| 名前 | `listing:name` | 文字化けしないZIP |
| サブタイトル | `listing:subtitles` | Windowsでも日本語が化けないZIP作成・解凍（採用 1 本だけ） |
| プロモーション用テキスト | `listing:promo` | 囲みの全文 |
| 説明 | `listing:description` | 囲みの全文 |
| キーワード | `listing:keywords` | カンマ区切り・空白なし |
| サポート URL | `listing:support-url` | `https://ukir-rrrr.github.io/winzip-jp/support/` |
| プライバシーポリシー URL | `listing:privacy-url` | `https://ukir-rrrr.github.io/winzip-jp/privacy/` |
| カテゴリ（第一） | 本文「カテゴリ案」 | ユーティリティ |
| カテゴリ（第二） | 同上 | 仕事効率化を付けるかは riku が決める |

スクリーンショットは `docs/store/screenshots/` の 2880×1800 を、Mac 16:10 に 5 枚。提出用の正は CI artifact `store-screenshots`（macos-15 / Hiragino）。リポジトリの PNG は Linux で作った参照。見出しは listing.md の表どおり。

サポート URL とプライバシー URL は GitHub Pages（公開フォルダ `/docs`）を riku が有効化したあとで生きる。Pages がまだなら、有効化してから提出する。

年齢制限は listing.md「年齢制限の回答」どおり。Made for Kids は **いいえ**。目安は 4+。

---

## 9. 輸出コンプライアンス（`ITSAppUsesNonExemptEncryption`）

本アプリは、ユーザが選んだときに **AES-256（WinZip AE-2）でユーザのファイルを暗号化する**。HTTPS のためだけではない。既定はオフだが、機能として入っている。

App Store Connect の質問と `apps/winzip-jp/src-tauri/Info.plist` のキーを揃える。今の Info.plist は **`true`**。これは実装の初期値であり、提出の最終回答ではない。最終は **riku が決める**。弁護士または [Apple の輸出コンプライアンス案内](https://developer.apple.com/documentation/security/complying-with-encryption-export-regulations) を見てよい。

### 選択肢

| | 意味 | 向いているとき |
| --- | --- | --- |
| **A. `false`（免除）** | 使う暗号化は免除対象だけ、と宣言する | HTTPS・認証・署名・OS のディスク暗号化だけ、と言えるとき |
| **B. `true`（非免除）** | 免除されない暗号化を使う、と宣言する | 今の Info.plist。AES-256 でユーザファイルを暗号化すると認める |
| **C. 1.0 から AES を外す** | パスワード付き ZIP を次版に送る | 暗号化質問を避けたいとき。設計上は MVP に AES を残している |

免除の例は通信の HTTPS、認証、デジタル署名、OS 標準のディスク暗号化。ユーザが選んだファイルを AES-256 で ZIP に書く用途は、よくある免除には当たりにくい。誤って **A** にすると、審査や輸出規制の手続きで問題になり得る。

**B** にすると、Connect の輸出コンプライアンス質問に答える。米国 BIS への年次自己分類報告（Annual Self Classification Report）が要ることがある。国によっては追加の書類がある。

**C** にするなら Info.plist を `false` にし、ポリシーテストも直し、AES UI を外す。このガイドの範囲外。

### 推奨（決定ではない）

**B（`true`）を推奨する。** 理由は次だけである。

- 製品はユーザファイルの AES-256 を実装している。通信の HTTPS だけ、とは言えない
- 今の Info.plist とポリシーテストは `true`
- 誤って `false` にする方が、あとの修正より重い

年次報告の手間を嫌うなら、弁護士に確認したうえで A にするか、C で AES を外す。エージェントはどれにも確定しない。

Connect 側の操作（B のとき）:

1. そのバージョンの **App Encryption Documentation** / 輸出コンプライアンス
2. 「輸出コンプライアンス情報は利用できますか」→ 使う暗号化がある、と進む
3. 免除対象だけか、と聞かれたら **いいえ**（ユーザファイルの AES-256）
4. 年次自己分類などの追加質問に、Apple の画面の説明どおり答える
5. Info.plist が `true` のままであることを確認してからビルドし直す（6. をやり直す）

A にしたときだけ、Info.plist の `ITSAppUsesNonExemptEncryption` を `false` に変えてから 6. をやり直す。

---

## 10. App Privacy（データ収集なし）

**App のプライバシー** を開く。

1. 「この App、または第三者のパートナーはデータを収集しますか」→ **いいえ**
2. トラッキングの質問が出たら → **いいえ**
3. データタイプの一覧が出ても、すべて収集しない（連絡先、位置情報、識別子、使用状況、診断、ユーザコンテンツなど）

根拠は `docs/store/listing.md`「App Privacy」と同じ。オフライン完結。解析 SDK なし。アプリ層の HTTP なし。アカウントなし。広告なし。`network.client` は WKWebView のローカル通信用で、利用者データを外に送らない。

---

## 11. 提出

ビルドがバージョンに付き、掲載文・スクリーンショット・価格・入手可能性・プライバシー・輸出コンプライアンスが揃ってから:

1. その macOS バージョンで **審査へ提出**
2. 輸出コンプライアンスの最終確認が出たら、9. で決めた答えと矛盾しないこと
3. 広告識別子は使っていない
4. 審査メモ（任意）の例:

```
日本語版 Windows 向けに CP932 の ZIP を作る Mac 専用アプリです。
パスワード（AES-256）と分割は初期オフです。オンにするとエクスプローラーでは開けません。
サンドボックス以外のファイルアクセスはありません。
```

提出後は App Store Connect の解決センターを見る。リジェクト理由が商標「WinZip」なら、表示名は「文字化けしないZIP」のまま、Bundle ID `jp.riku.winzip` は変えない（設計 §9.1）。

---

## やってはいけないこと

- 証明書、`.p12`、`.p8`、`.cer`、`.certSigningRequest`、`.provisionprofile`、パスワード、API キーを `git add` する
- `apps/winzip-jp/scripts/appstore-pkg.sh` を upload するよう改変する（スクリプトは `--no-upload`）
- EU を販売対象にする（DSA 非トレーダー）
- Intel / ユニバーサルバイナリを `--target universal-apple-darwin` で作る
- Developer ID 証明書でストア用 `.pkg` に署名する
- `codesign --deep` を付ける（WKWebView ヘルパーが壊れる）

---

## 失敗したとき

| 症状 | 見ること |
| --- | --- |
| スクリプトが ad-hoc と出る | `security find-identity -v -p codesigning` に `Apple Distribution` と `CHSRTG92SZ` が無い |
| プロファイルが無いと落ちる | `APPSTORE_PROVISION_PROFILE` のパス。ファイルを git に入れていないか |
| Transporter が署名エラー | `.pkg` が Mac Installer Distribution で署名されているか。`pkgutil --check-signature` |
| Connect にビルドが出ない | 同じチームか。Bundle ID が `jp.riku.winzip` か。待ってから再読み込み |
| サンドボックスで即終了 | 提出用ではなく `sandbox-e2e.sh` の話。ad-hoc に Team ID が残ると SIGKILL する |
