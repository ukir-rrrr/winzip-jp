# Mac App Store 掲載文（日本ストア）

提出先は日本の Mac App Store。販売者タイプは個人。価格は買い切り **¥900**（有料アプリ。非消費型 IAP にはしない）。
本文はすべて日本語。英語の併記はしない。

文字数の検証は JS の `string.length`（UTF-16 コードユニット）。App Store Connect の表示も同じ目安として扱う。絵文字は使わない。検証は `node docs/store/check-listing.mjs`。

スクリーンショットの再生成:

```bash
VITE_E2E=1 pnpm --filter winzip-jp build
pnpm --filter winzip-jp store-screenshots
```

出力は `docs/store/screenshots/`（2880×1800）。リポジトリに置く PNG は Linux（Noto Sans CJK JP）で作った。CI の macos-15（Hiragino）で作り直したものを artifact `store-screenshots` に上げ、提出用はそちらを正とする。CI はサイズだけ見る。フォントが違うのでピクセル一致は見ない。

---

## アプリ名（≤30）

<!-- listing:name -->
文字化けしないZIP
<!-- /listing:name -->

## サブタイトル（採用・提出する 1 本、≤30）

riku が採用した。提出はこの 1 本だけ。

<!-- listing:subtitles -->
Windowsでも日本語が化けないZIP作成・解凍
<!-- /listing:subtitles -->

## プロモーション用テキスト（≤170）

<!-- listing:promo -->
Windowsの相手が追加ソフトなしで開けるZIPを、Macで作れます。Apple Silicon（M1〜M4）対応。.DS_Storeと__MACOSXは自動で除きます。文字化けZIPの解凍もできます。パスワードと分割は先方に7-Zipなどが必要です。
<!-- /listing:promo -->

## 説明（≤4000）

先頭は次をこの順で述べる。Windowsの相手がそのまま開ける／追加ソフト不要／Apple Silicon M1–M4対応／.DS_Store と __MACOSX を除外／文字化けZIPの解凍。パスワードと分割は、受け取る側に 7-Zip などが必要だと明記する。

<!-- listing:description -->
Windowsの取引先が、エクスプローラーの標準機能だけでそのまま開けるZIPを、Macで作れます。追加の解凍ソフトは不要です。Apple Silicon（M1、M2、M3、M4）のネイティブアプリです。Rosettaは使いません。.DS_Storeと__MACOSXは、どの階層にあってもZIPに入れません。Windowsから届いた文字化けZIPも、文字コードを切り替えて一覧を確認してから解凍できます。

ファイルやフォルダをドロップするか選ぶと、1つのZIPになります。ファイル名は日本語Windowsが読む形式（CP932）で書き、Finderが付ける分解文字（濁点の分離）は先に合成します。Windowsで使えない文字（一部の人名漢字や絵文字）は、置き換えを確認してから作ります。「このまま作る」はありません。

解凍では、アーカイブ全体の文字コードを自動で判定します。自信が低いときは切替を強調します。CP932とUTF-8を手元で切り替えると、一覧の漢字がすぐに変わります。読める方を選んでから展開してください。

パスワード（AES-256）と分割は初期状態ではオフです。どちらかを付けると、Windowsのエクスプローラーでは開けません。受け取る人に7-Zip、Lhaplus、WinZipなどが必要です。相手がWindows標準だけで開くなら、パスワードも分割も付けないでください。日本語のパスワードは、先方もUTF-8前提のツールに限ります。

このアプリはオフラインで完結します。入力ファイルを外部に送りません。アクセスするのは、選んだ場所とドロップした場所だけです。画面は日本語のみです。

含めないもの: Intel Mac、4GiBを超えるZIPの作成、自己解凍EXE、RARやISOの作成、複数言語の画面。パスワード付き・分割ZIPを「エクスプローラーで開ける」とは言いません。
<!-- /listing:description -->

## キーワード（≤100、カンマ区切り、空白なし）

アプリ名と採用サブタイトルの語（文字化け／しない／ZIP／Windows／日本語／化けない／作成／解凍）は繰り返さない。

<!-- listing:keywords -->
圧縮,アーカイブ,CP932,ShiftJIS,エクスプローラー,Mac,ファイル名,暗号化,分割,Finder,パスワード,展開,取引先,オフライン
<!-- /listing:keywords -->

## カテゴリ案

- 第一: ユーティリティ（`public.app-category.utilities`。Info.plist 済み）
- 第二（任意）: 仕事効率化（`public.app-category.productivity`）

第二カテゴリを付けるかは riku が決める。

## サポート URL / プライバシーポリシー URL

本文は `docs/support/` と `docs/privacy/`（GitHub Pages の公開フォルダを `/docs` にしたとき、次の URL になる）。この PR では Pages を有効化しない。有効化は riku がする。

<!-- listing:support-url -->
https://ukir-rrrr.github.io/winzip-jp/support/
<!-- /listing:support-url -->

<!-- listing:privacy-url -->
https://ukir-rrrr.github.io/winzip-jp/privacy/
<!-- /listing:privacy-url -->

## App Privacy（データ収集なし）

App Store Connect の「App Privacy」:

- この App、または第三者のパートナーはデータを収集しますか → **いいえ**
- トラッキングしますか → **いいえ**（質問が出る場合）

収集しないデータの例（質問にデータタイプが出ても、すべて収集しない）:

- 連絡先情報、健康、財務、位置情報
- ユーザコンテンツ、検索履歴、閲覧履歴
- 識別子、購入、使用状況、診断
- 周囲、身体、その他のデータ

根拠: オフライン完結。解析 SDK なし。アプリ層の HTTP クライアントなし。アカウントなし。広告なし。パスワードは処理後に画面側の値を空にする。ログに出さない。JavaScript の文字列の完全消去は保証しない。network.client は WKWebView のローカル通信用で、利用者データの送信には使わない。Apple および OS はそれぞれの方針で収集することがある。

## 年齢制限の回答

Made for Kids → **いいえ**

頻度はすべて **なし**。Web / 賭博系は **いいえ**。

| 項目 | 回答 |
| --- | --- |
| 無制限の Web アクセス | いいえ |
| ギャンブル | いいえ |
| コンテスト | いいえ |
| 漫画・ファンタジーの暴力 | なし |
| リアルな暴力 | なし |
| 長時間の陰惨なリアルな暴力 | なし |
| 不適切な言葉や下品なユーモア | なし |
| アダルト / 性的なテーマ | なし |
| ホラー / 恐怖 | なし |
| 医療・治療情報 | なし |
| アルコール、タバコ、薬物の使用または言及 | なし |
| 模擬ギャンブル | なし |
| 性的内容やヌード | なし |
| 露骨な性的内容やヌード | なし |
| 銃やその他の武器 | なし（質問がある場合） |

結果の目安は **4+**。最終ラベルは Connect が出す。

## 輸出コンプライアンス（App Store Connect で答える）

本アプリは、ユーザが選んだときに **AES-256（WinZip AE-2）でファイルを暗号化する**。HTTPS のためだけではない。

`apps/winzip-jp/src-tauri/Info.plist` に `ITSAppUsesNonExemptEncryption` は置かない。`true` を付けると Transporter Error 90592（Invalid Export Compliance Code）になる。回答は App Store Connect の輸出コンプライアンス質問で行う。手順は `docs/release/submit-guide.md` の §9。Connect での文言は riku が決める。

## スクリーンショット（2880×1800）

Mac で受理される 16:10。実アプリ UI（`VITE_E2E=1` のプレビュー）を撮り、短い日本語見出しを載せる。うその機能は書かない。

| ファイル | 見出し |
| --- | --- |
| `screenshots/01-windows.png` | Windowsでも文字化けしない |
| `screenshots/02-drop.png` | ドロップするだけでZIP作成 |
| `screenshots/03-extract.png` | 文字化けZIPもきれいに解凍 |
| `screenshots/04-junk.png` | .DS_Storeなど不要ファイルを自動除外 |
| `screenshots/05-extract-drop.png` | 文字化けしたZIPをドロップして解凍 |

## riku が決めること

1. GitHub Pages を有効化するか（公開フォルダ `/docs`。この PR では有効化しない）。
2. App Store Connect の輸出コンプライアンス質問への回答。Info.plist に `ITSAppUsesNonExemptEncryption=true` は置かない。
3. 第二カテゴリ（仕事効率化）を付けるか。
