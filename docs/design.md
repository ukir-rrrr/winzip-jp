# winzip-jp 設計書

日本語版 Windows とやり取りする会社員向けの、Mac 専用買い切りアプリ。
作る ZIP は日本語ファイル名が Windows エクスプローラーで化けない。受け取った文字化け ZIP は正しく解凍できる。

---

## 統合メモ

### ベースと理由

**ベースは候補 B（grok46）**。この製品の核は「Windows に渡したファイル名バイト列が正しいこと」であり、B はその不変条件・検証・段階計画が実装可能な粒度で揃っている。A（Composer）は文書が短く読みやすいが、中核の writer と Windows CI の前提が事実と食い違う。拡張性でも、作成側ヘッダを自前に置く B の方が、後から UTF-8 上級オプションや AES extra を足してもフラグ契約を壊しにくい。

### 採点（各 10 点）

| 評価基準 | A Composer | B grok46 | 根拠（事実確認済み） |
| --- | ---: | ---: | --- |
| Windows 出力の正しさ | 6 | 10 | zip 9.x は raw 名が valid UTF-8 かつ非 ASCII なら **bit 11 を自動で立てる**。公開の opt-out は無い。A は `zip` crate の writer に預け、この挙動に触れていない。B は自前ヘッダで bit 11 = 0 を固定する。 |
| サンドボックス適合 | 7 | 9 | entitlements は両候補とも正しい。`tauri-plugin-scoped-bookmarks` は **実在する**（crates.io 0.1.0）が、ダウンロード数十・第三者・2026-08 公開。A は MVP から永続 grant。B はセッション内の user-selected / D&D だけに閉じ、`persisted-scope` がカーネル許可を復元しない点も正確。 |
| テスト・画面検証のしやすさ | 4 | 10 | GitHub `windows-latest` の ACP は **en-US / 1252**。A の「Expand-Archive の結果を CP932 で再エンコードして比較」は oracle にならない。B は raw バイト比較 + 7-Zip `-mcp=932` を正とし、Expand-Archive は bit 11 誤爆の **負のスモーク**にする。macOS E2E も A の `tauri-driver` は WKWebView を駆動できない。B の embedded WebDriver が現行の正道。A の `macos-14` は 2026-11-02 廃止（本日起算で既に brownout 期間）。 |
| シンプルさ | 8 | 6 | A は短い。ただし writer を crate に預ける簡潔さは、bit 11 問題を先送りする偽の簡単さ。B は文書が長いが、モジュールは encoding / write / read / split の 4 つ、画面は 3 つ。 |
| 段階計画の質 | 6 | 9 | B は「エンコーディング → ZIP バイト → Windows oracle → UI」の順で、各 PR が CI で証明できる。A は PullFrog を PR-9、画面検証が PR-4 以降に偏り、400 行制限が根拠薄い。 |
| **合計** | **31** | **44** | |

### 移植した点と出典

| 移植 | 出典 | 入れ方 |
| --- | --- | --- |
| VoiceOver で主要操作が辿れる | A §1.3 NF-05 | 非機能 N9 |
| MVP はオフライン完結・外部送信なし | A §1.3 NF-04 | 非機能 N10 |
| v1.1: Finder Quick Action、最近使った出力先 | A §1.4 | 後回し表 |
| v1.2: 7z 読み取り、バッチキュー | A §1.4 | 後回し表 |
| Tauri fs scope とカーネル許可の二重チェック | A §4.2 | サンドボックス §4.5 |
| MAS では D&D 後も security-scoped を明示開始 | A §4.3 / B §4.2 | 統合して §4.2 |
| 圧縮開始ハングの回帰（起動直後に progress） | A NF-03 / B N4 | N4 に 2 秒以内で固定 |
| `plan`（作成前分析）を UI の正式ステップにする | A `analyze_sources` / B `plan_write` | B の API 名で統一 |
| ゴールデン更新はレビュー必須 | A §5.4 | CI §5.3 |
| 価格はレンジのまま、推奨値だけ置く | 元プロンプト ¥980〜1,500 / B の ¥1,280 | 未決に残し推奨のみ |

### 却下した点

| 却下 | 出典 | 理由 |
| --- | --- | --- |
| `zip` crate の writer を作成の正とする | A §2 | zip 9.0.3 の `initialize_local_block` / `get_flags` が valid UTF-8 非 ASCII 名に Language Encoding Flag を立てる。CP932 列は偶然 valid UTF-8 になり得る。公開 API で bit 11 を落とす口が無い。 |
| Expand-Archive / Shell.Application / tar をファイル名の主 oracle にする | A §5.3、元プロンプト例 | ランナー ACP=1252 では CP932 名を正しく復元しない。補助（構造・bit 11 誤爆）に格下げ。 |
| `macos-14` | A §5.2 | 2026-11-02 廃止。最初から `macos-15`（Apple Silicon）。 |
| `tauri-driver` を macOS E2E の主手段にする | A §6.2 | 公式は macOS で embedded（`tauri-plugin-wdio-webdriver` + `@wdio/tauri-service`）。`tauri-driver` は Windows/Linux。 |
| MVP で `tauri-plugin-scoped-bookmarks` による永続 grant | A §4.2 | crate は実在するが新規・低採用。MVP は毎回ダイアログ/D&D。最近使った出力先は v1.1。 |
| `._*` AppleDouble を MVP 除外に含める | A F-03 | 元プロンプトの除外は `.DS_Store` と `__MACOSX` のみ。`._*` は v1.1。 |
| PR 差分 400 行制限 | A §8 | ゴールデンや entitlements で無意味に分割される。完了条件は CI 証明。 |
| nightly ffmpeg 録画 | A §6.4 | コストに対して情報が増えない。PNG + pixelmatch で足りる。 |
| PullFrog を `pull_request` ワークフローだけにする | A §7 | 公式・参照リポジトリ（`ukir-rrrr/my-blog-site/.github/workflows/pullfrog.yml`）は **`workflow_dispatch` のみ**。コンソールが PR 作成/push で dispatch する。Arena 要件の `pull_request` は別ファイルで足し、コンソール「Review PRs」はオフ（二重投稿防止）。 |
| 作成側 ZIP64 / Unicode Path (0x7075) 併記 / ZipCrypto | 両候補の後回し、A は 0x7075 を書かないで一致 | 1.0 では書かない。 |
| ピクセル差 0.2% | B §6.3 | タイトルバーやフォント raster でフレークしやすい。コンテンツ領域に切り、許容は **1%**（A の目安に合わせる）。 |

作成ヘッダの自前実装は重いが、製品の売りそのものなので残す。将来 `zip` crate が `with_utf8(false)` 相当を公開したら、ゴールデン（`name_hex` + `gpbf`）を通したうえで writer 実装だけ差し替えてよい。読み取りと AES 復号は引き続き `zip` crate。

---

## 1. 要件

### 1.1 対象と成功条件

対象は、Windows の取引先・同僚に ZIP を送る日本の会社員。自分は Mac。

成功の定義:

> このアプリで作った ZIP を、日本語版 Windows 11 のエクスプローラーが標準機能だけで解凍したとき、ファイル名が送り側 Mac の表示と一致する（NFC、CP932 で表せる文字の範囲）。

AES 付き・分割付きは「先方が 7-Zip / Lhaplus / WinZip を持つ場合のオプション」であり、エクスプローラー互換の成功定義からは外す。UI 上もそう書く。

差別化: Apple Silicon（M1〜M4）ネイティブで、圧縮開始が必ず起きる（競合 WinZipper の M4 レビューへの対抗）。Rosetta なし。C の zlib/lzma をリンクしない。

価格帯: Mac App Store 買い切り **¥980〜1,500**。推奨表示価格は ¥1,280（未決。§9）。

### 1.2 MVP（1.0 に入れる）

| ID | 機能 | 完了の見た目 |
| --- | --- | --- |
| F1 | ドラッグ&ドロップおよびファイル選択で ZIP 作成 | ドロップしたファイル・フォルダが 1 つの ZIP になる |
| F2 | エントリ名を CP932（Windows-31J）で書き、EFS（bit 11）を立てない | ゴールデン ZIP とバイト一致。Windows 7-Zip `-mcp=932` で名前一致 |
| F3 | パス区切りは `/`。ホスト OS は MS-DOS (0) | エクスプローラーが Unix パーミッションを見ない |
| F4 | NFD → NFC | `が` が分解文字のまま CP932 化されない |
| F5 | `.DS_Store` と `__MACOSX` を除外 | どの階層でも除外。ZIP 内に現れない |
| F6 | CP932 不能文字の警告と置換案 | 作成前に一覧。置換を確定しないと作れない |
| F7 | 任意 AES-256（WinZip AE-2）パスワード | オフがデフォルト。オン時は「エクスプローラーでは開けない」と明示 |
| F8 | 任意分割（WinZip 形式 `.z01`…`.zip`） | デフォルトオフ。サイズは 8 / 10 / 25 / 100 MB と任意 |
| F9 | 文字化け ZIP の解凍。CP932 / UTF-8 自動判定 + 手動切替 | 一覧のプレビューが切替ですぐ変わる |
| F10 | Finder らしい日本語 UI | 英語が出ない。作成 / 解凍の 2 モード |
| F11 | キャンセル可能な進捗 | 作成・解凍とも中断で部分ファイルを捨てる |

### 1.3 非機能（MVP）

| ID | 要件 |
| --- | --- |
| N1 | MAS サンドボックス。アクセスはユーザーが選んだ（またはドロップした）場所だけ |
| N2 | Apple Silicon ネイティブ。Rosetta なし。圧縮は純粋 Rust |
| N3 | 圧縮開始が M シリーズで必ず起きる |
| N4 | 作成 2 秒以内に最初の進捗イベント（小さなファイルは完了まで） |
| N5 | パスワードはメモリ上で保持し、終了時にゼロ化。ログに出さない |
| N6 | 解凍時パストラバーサル禁止（`..`、絶対パス、バックスラッシュ） |
| N7 | クラッシュしても入力ファイルを消さない。出力は一時ファイルから rename |
| N8 | UI は macOS 15 の GitHub Actions 上でスクリーンショット回帰 |
| N9 | VoiceOver で作成・解凍・キャンセルが辿れる（最低限） |
| N10 | MVP はオフライン完結。解析用の外部送信なし。アプリ層の HTTP は持たない。WKWebView のローカル IPC に必要な `network.client` 以外のネットワーク entitlement は付けない |

### 1.4 明示的に後回し（1.0 に入れない）

| 時期 | 内容 |
| --- | --- |
| **v1.1** | Finder Quick Action、最近使った出力先（そのとき `tauri-plugin-scoped-bookmarks` を再評価）、`._*` AppleDouble 除外、圧縮レベルテンプレート、「UTF-8 で書く」上級オプション |
| **v1.2** | 7z 読み取り、バッチキュー、社内向け CLI |
| **見送り** | Intel Mac / ユニバーサルバイナリ、作成側 ZIP64（4 GiB 超または 65535 エントリ超はエラー）、Info-ZIP Unicode Path (0x7075) 併記、ZipCrypto、自己解凍 EXE、ISO、RAR 作成、iCloud、複数言語 UI、フルディスクアクセス、ホームディレクトリ包括許可、Windows 版 |

### 1.5 受け入れの数値目標（1.0）

- ゴールデン（最低 20 本）のファイル名バイト列と `gpbf` が 100% 一致
- Windows ジョブの 7-Zip oracle が Unicode 名 100% 一致
- E2E スクリーンショットがベースラインとコンテンツ領域で 1% 超差なら失敗
- サンドボックス entitlements 以外のファイル entitlement を持たない

---

## 2. アーキテクチャとモジュール境界

### 2.1 方針

ZIP の **ファイル名とフラグは自前で書く**。Deflate と CRC と AES の中身は既存 crate を使う。

理由（現行 zip 9.0.3 で確認）:

- `ZipFileData::initialize_local_block` は `std::str::from_utf8(file_name_raw).is_ok() && !file_name_raw.is_ascii()` のとき `ZipFlags::LanguageEncoding`（bit 11）を立てる
- `get_flags` も同じ判定をヘッダ書き出し時に再適用する
- `start_file_with_options` は非 UTF-8 名を受け付けるが、**valid UTF-8 に見える CP932 列の bit 11 を落とす口は無い**
- 読み取りは圧縮形式・ZIP64・AES・壊れたアーカイブの動物園なので `zip` crate に任せる

Windows の CI で WebKit をビルドしないため、コアは Tauri から独立した crate にする。

### 2.2 リポジトリ配置

```
crates/zipcore/          # エンコーディング・作成・解凍。macOS/Windows とも cargo test
apps/winzip-jp/          # Tauri v2 + React + TypeScript
  src/                   # UI
  src-tauri/             # コマンド、entitlements、サンドボックス
  e2e/                   # WebdriverIO
tests/golden/            # 入力ツリーと期待 name_hex / gpbf
tests/oracles/           # inspect_zip.py / windows-extract.ps1
.github/workflows/
  ci.yml
  pullfrog.yml           # 公式 workflow_dispatch
  pullfrog-review.yml    # Arena 要件の pull_request
.pullfrog/review.md
```

パッケージマネージャは骨格 PR で **pnpm** に固定する（lockfile を 1 本にする）。

```
React UI  --invoke-->  Tauri commands  -->  sandbox.rs
                                    \-->  zipcore
                                           encoding / write / read / split
```

### 2.3 `zipcore` の API

モジュールは 4 つ。`encoding` / `write` / `read` / `split`。`split` は出力ストリームを差し替え、分割時だけ EOCD / 中央ディレクトリにディスク番号とディスク相対オフセットを書く（7-Zip は `ThisDisk == 0` だと最後の `.zip` だけを単体として開く）。非分割のヘッダ契約（CP932 `name_raw`、bit 11 = 0、disk = 0）は変えない。

```rust
// crates/zipcore/src/encoding.rs

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NameEncoding { Cp932, Utf8, Cp437 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unmappable {
    pub relative_path: String,   // NFC、入力側の表示名
    pub original_char: String,
    pub suggested: String,
    pub kind: UnmappableKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnmappableKind { CompatIdeograph, Emoji, Other }

pub struct EncodedName {
    pub cp932: Vec<u8>,          // '/' 区切り。末尾 '/' はディレクトリ
    pub issues: Vec<Unmappable>,
}

pub fn encode_entry_name(unicode_path: &str) -> EncodedName;

pub fn detect_encoding(entries: &[(u16 /*gpbf*/, Vec<u8> /*raw name*/, Vec<u8> /*extra*/)])
    -> Detection;

pub struct Detection {
    pub encoding: NameEncoding,
    pub confidence: f32,         // 0.0–1.0。0.6 未満なら UI で切替を強調
    pub ambiguous: bool,
}

pub fn decode_name(raw: &[u8], encoding: NameEncoding) -> String;
```

```rust
// crates/zipcore/src/write.rs

pub struct WriteRequest {
    pub inputs: Vec<InputItem>,
    pub dest: PathBuf,
    pub password: Option<SecretString>,
    pub aes: AesMode,            // 既定 Aes256。password があるときだけ使用
    pub split_bytes: Option<u64>,
    pub replacements: BTreeMap<String, String>,
}

pub struct InputItem {
    pub abs_path: PathBuf,
    pub as_dir: bool,
}

pub fn plan_write(req: &WriteRequest) -> Result<Plan, ZipcoreError>;
pub fn write_zip(
    req: WriteRequest,
    progress: impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<WriteReport, ZipcoreError>;
```

書き込みの不変条件（ユニットテストで固定する）:

1. ローカルヘッダ・中央ディレクトリのファイル名バイトが `encode_entry_name` の `cp932` と一致
2. general purpose bit flag の bit 11 が **全エントリで 0**
3. bit 0 は非暗号なら 0、AES なら 1
4. extra field に 0x7075 を**書かない**
5. ホスト OS バイト = 0（MS-DOS）
6. 圧縮メソッド = 8（Deflate）。ディレクトリは 0（Stored）+ 名前末尾 `/`

```rust
// crates/zipcore/src/read.rs

pub fn list_zip(req: ListRequest) -> Result<ListReport, ZipcoreError>;
pub fn extract_zip(
    req: ExtractRequest,
    progress: impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<ExtractReport, ZipcoreError>;
```

解凍の不変条件:

- `display_name` に `..` 成分があればエラー（スキップしない。アーカイブごと失敗）
- 出力パスは `dest.canonicalize()` 配下に正規化後も留まっていること
- 既存ファイルは上書き確認を UI に返す（`extract_zip` は `overwrite: bool`）

### 2.4 Tauri コマンド

フロントはファイル内容を読まない。パスと設定だけ渡す。

```rust
#[tauri::command]
fn plan_archive(paths: Vec<String>, dest_name_hint: Option<String>) -> Result<PlanDto, String>;

#[tauri::command]
async fn create_archive(window: Window, req: CreateDto, dest: String) -> Result<WriteReportDto, String>;

#[tauri::command]
fn list_archive(path: String, encoding: Option<NameEncoding>) -> Result<ListReportDto, String>;

#[tauri::command]
async fn extract_archive(window: Window, req: ExtractDto) -> Result<ExtractReportDto, String>;

#[tauri::command]
fn cancel_job();

#[tauri::command]
fn pick_files() -> Result<Vec<String>, String>;

#[tauri::command]
fn pick_save_zip(default_name: String) -> Result<Option<String>, String>;

#[tauri::command]
fn pick_extract_dir() -> Result<Option<String>, String>;
```

進捗はイベント:

```ts
await listen<ProgressDto>('zip://progress', handler);
```

`create_archive` / `extract_archive` は `spawn_blocking`。ジョブ開始直後に `progress { phase: "starting" }` を出す（N3/N4）。

### 2.5 フロントの境界

React は 3 画面だけ。

- `CreateScreen`: ドロップ、一覧、不能文字ダイアログ、オプション（AES / 分割）、保存
- `ExtractScreen`: ZIP ドロップ、エンコーディング切替、一覧プレビュー、展開先
- `ProgressOverlay`: 共通

状態は Zustand 1 ストア。Rust が返した DTO をそのまま持つ。エンコーディング判定を JS でやり直さない。

#### UI ライブラリとテーマ

UI ライブラリは **Mantine 最新安定版**（導入時点 9.7.x）。`@mantine/core` と `@mantine/hooks` だけを入れる。dates / charts / notifications / carousel などは使わない（バンドルサイズ）。コンポーネントは named import。CSS は使う部品だけ読む。Hello 時点は `baseline` / `default-css-variables` / `global` / `UnstyledButton` / `Title` の 5 ファイル。

テーマは macOS / Finder に寄せる。

| 項目 | 方針 |
| --- | --- |
| フォント | システムフォント。`-apple-system, BlinkMacSystemFont, "Hiragino Sans", "Hiragino Kaku Gothic ProN", "Yu Gothic UI", sans-serif`。Web フォントを取らない |
| カラー | `MantineProvider` の `defaultColorScheme="auto"` で OS のライト / ダークに追従。面とアクセントは `theme.ts` の `cssVariablesResolver` が唯一の定義。アクセントは macOS の `#007AFF` / `#0A84FF`（`macos` スケール 5 / 4） |
| 言語 | 日本語のみ。英語フォールバック文字列をコンポーネントに残さない。`html lang="ja"` |
| 見た目 | ツールバーとドロップウェル。線や影で区切らず、面のトーン差。ルートに `data-ui` |

UI を触るエージェントは taste-skill に従い、ブラウザまたは画面で確認する（`AGENTS.md`）。ランディング向けの方向抽選や写真フロアは適用しない。

### 2.6 依存 crate

| crate | 用途 | 入れない理由があるもの |
| --- | --- | --- |
| `encoding_rs` | CP932 = `SHIFT_JIS`（Windows-31J） | 旧 `encoding` crate は使わない |
| `unicode-normalization` | NFD→NFC | — |
| `flate2`（default、miniz_oxide） | Deflate | `zlib-ng` を足さない |
| `crc32fast` | CRC-32 | — |
| `zip` `default-features=false` + `deflate` + `aes-crypto` | **読み取り**と AES 復号 | bzip2/lzma/xz/zstd の C 依存を切る |
| `aes` / `hmac` / `pbkdf2` / `sha1` | AE-2 の**書き込み** | `zip` の writer にファイル名を預けない |
| `zeroize` | パスワード | — |

`zip` の writer は使わない。reader と AES 復号だけ借りる。

---

## 3. エンコーディング戦略

### 3.1 作成（Windows 向け）

日本語版 Windows エクスプローラーは、bit 11 が落ちているファイル名を **ACP = CP932** として読む。仕様上のフォールバックは CP437 だが、日本語 Windows の実装は自国コードページである。

作成時は次で固定する。

| 項目 | 値 |
| --- | --- |
| ファイル名バイト | CP932（`encoding_rs::SHIFT_JIS`） |
| EFS bit 11 | 常に 0 |
| 0x7075 Unicode Path | 書かない |
| パス区切り | `/`（APPNOTE。`\` は書かない） |
| 正規化 | 書き込み直前に NFC |
| ASCII のみの名前 | それでも bit 11 は 0（挙動を分岐させない） |

UTF-8 + bit 11 は現代のエクスプローラーでは読めることが多い。それでも採用しない。先方の「右クリック → すべて展開」と、社内に残る古い Lhaplus / 添付ファイルゲートウェイが CP932 前提だからである。

### 3.2 正規化と不能文字

処理順:

1. 相対パスを `/` に正規化（入力の `\` も区切りとみなし、issues に記録。macOS では `\` はファイル名文字になり得る）
2. 各パス成分を `nfc()`
3. 成分ごとに CP932 へ `encode_without_replacement`
4. 不能コードポイントは置換表、無ければ `_`
5. 成分が空、`.`、`..` なら、置換の有無にかかわらず `_`（ZIP 名にパストラバーサル成分を残さない）
6. 同一 ZIP 内で CP932 バイト列が衝突したら ` (2)` ` (3)` を NFC 側に足して再エンコード

既定の置換表:

| 入力 | 置換 | 理由 |
| --- | --- | --- |
| U+9AD9 髙 | 高 | 人名に多い。CP932 外 |
| U+FA11 﨑 | 崎 | 同上 |
| U+FA0E 﨎 など IBM 拡張外の互換文字 | 対応する統合漢字 | 表は `encoding.rs` に定数 |
| 絵文字・国旗 | `_` | 推測しない |

UI は「このまま作る」を出さない。一覧で置換を確認・編集してから作成する。編集後も CP932 不能ならその場で再警告。セッションをまたいだ置換の記憶は v1.1。

### 3.3 Mac ジャンク

除外判定はパス成分単位。

```
fn is_junk(rel: &Path) -> bool {
    rel.components().any(|c| {
        let s = c.as_os_str();
        s == ".DS_Store" || s == "__MACOSX"
    })
}
```

フォルダをドロップした場合、中の `.DS_Store` も除外する。ユーザーが「`.DS_Store` そのもの」を単体ドロップしたら同様に除外し、結果が空ならエラー「追加できるファイルがありません」。

### 3.4 解凍（判定）

エントリ単位ではなくアーカイブ単位で 1 つの読み方を決める。混在 ZIP は手動切替に倒す。

```
function detect(entries):
  if 過半数のエントリで bit 11 が立っている:
      return Utf8, confidence=0.95
  extra 0x7075 が CRC 一致で過半数:
      return Utf8, confidence=0.9
  raw がすべて ASCII:
      return Cp932, confidence=0.5
  utf8_ok = raw が valid UTF-8 の非 ASCII エントリ数
  cp932_ok = encoding_rs 置換なしで通る非 ASCII エントリ数
  if cp932_ok > utf8_ok: return Cp932
  if utf8_ok > cp932_ok: return Utf8
  return Cp932, confidence=0.4, ambiguous=true   # 主用途は Windows 由来
```

Finder は UTF-8 なのに bit 11 を立てない世代がある。だから「bit 11 なし = 必ず CP932」にはしない。

曖昧（`confidence < 0.6` または `ambiguous`）のときは切替を強調し、一覧プレビューで漢字が読めるかをユーザーが見る。切替は即 `list_archive`。ファイルはまだ書かない。

手動選択肢は **CP932 / UTF-8** の 2 つだけ。CP437 は自動の内部フォールバックに残すが UI に出さない。

### 3.5 パスワードのバイト

AES のパスワードは **UTF-8 バイト**（WinZip / 7-Zip と同じ）。日本語パスワードは「先方も UTF-8 前提のツール」に限る、とオプション説明に 1 行書く。

---

## 4. サンドボックス戦略

### 4.1 Entitlements（これ以外足さない）

`apps/winzip-jp/src-tauri/Entitlements.plist`:

```xml
<key>com.apple.security.app-sandbox</key><true/>
<key>com.apple.security.files.user-selected.read-write</key><true/>
<key>com.apple.security.files.bookmarks.app-scope</key><true/>
<key>com.apple.security.network.client</key><true/>
<key>com.apple.application-identifier</key>
<string>CHSRTG92SZ.jp.riku.winzip</string>
<key>com.apple.developer.team-identifier</key>
<string>CHSRTG92SZ</string>
```

`com.apple.security.files.downloads.read-write` やホーム包括は付けない。一時ファイルはコンテナの `NSTemporaryDirectory` だけ。`network.server` は付けない。`network.client` は WKWebView の WebContent がローカル IPC に outgoing を要求するためだけで、アプリ層の HTTP クライアントは持たない（N10）。

AES を含むため `ITSAppUsesNonExemptEncryption` は提出前に確定する（§9）。開発中は `false` で進める。

### 4.2 アクセスが開く経路は 2 つだけ

1. **NSOpenPanel / NSSavePanel**（`tauri-plugin-dialog`）
2. **ドラッグ&ドロップ**（Tauri の `DragDropEvent::Drop`）

どちらもカーネルがそのセッションの security-scoped を付ける。Rust 側はパス文字列を受け取ったら、すぐ `startAccessingSecurityScopedResource` 相当を呼ぶ。MAS では D&D だけでは足りないことがあるので、ドロップ後も明示的に開始する。

`tauri-plugin-persisted-scope` は **Tauri の allowlist を永続化するだけ**で、カーネルのサンドボックス許可は復元しない。使わない。

MVP では起動をまたいでパスを覚えない。毎回ドロップかダイアログ。v1.1 の「最近使った出力先」で `tauri-plugin-scoped-bookmarks` を再評価する（実在は確認済み。採用は依存の成熟を見て決める）。

### 4.3 作成フローとスコープ寿命

```
drop/open → startAccessing(inputs)
pick_save_zip → startAccessing(dest ファイル)
コンテナ内 tmp.zip に write_zip
fsync → dest へ rename
  別ボリューム（EXDEV）だけ copy + remove。既存 dest はコンテナへ退避してから置き、失敗したら戻す。
  dest を fsync してから退避を消す。戻しに失敗したら退避は消さない。
  新規 dest で fsync に失敗したら dest を消してエラー。
  エラーは種別の日本語と退避パス。OS の生文言は出さない。親フォルダに sidecar は作らない。
失敗・キャンセル → tmp 削除
stopAccessing(all)
```

入力ディレクトリの中を再帰 walk してよいのは、ドロップ/選択したそのディレクトリに対するスコープがある場合だけ。シムリンクでスコープ外へ出たらそのエントリをスキップして警告。

### 4.4 解凍フロー

```
drop/open ZIP → startAccessing(zip)
pick_extract_dir → startAccessing(dest)
コンテナに展開せず、dest 配下へ直接書く
  （各ファイルは dest/.winzip-jp-tmp-* 経由で rename）
stopAccessing
```

巨大 ZIP をコンテナに二重書きしない。

### 4.5 Tauri 側の二重チェック

- `tauri-plugin-fs` のスコープは `$TEMP`（コンテナ）と、コマンド引数で渡ってきたパスだけ。`$HOME/**` をコンフィグに書かない
- コマンド層で「今回 startAccessing したルートの配下か」を検証してから `std::fs`
- 開発用 `tauri.conf.json` はサンドボックスなしでも起動できる
- 提出用 `tauri.appstore.conf.json` だけが entitlements と `signingIdentity`（`Apple Distribution`。Team `CHSRTG92SZ` は `appstore-pkg.sh` が解決）を読む。プロビジョニングプロファイルはリポジトリに置かず、提出時だけローカルパスで埋め込む

---

## 5. テスト / CI 計画

### 5.1 原則

**ファイル名の正しさは Unicode 文字列比較ではなく、ZIP 内 raw バイトと比較する。**

GitHub の `windows-latest` / `windows-2022` は en-US で ACP が 1252 のため、`Expand-Archive` や `Shell.Application` が日本語名を正しく復元することを期待してはいけない。それらは「壊れていないか」「誤って bit 11 を立てていないか」の補助であり、名前の oracle ではない。

### 5.2 レイヤ

| レイヤ | 場所 | ランナー |
| --- | --- | --- |
| 単体 | `crates/zipcore` | macos-15, windows-2022, ubuntu-24.04 |
| ゴールデン | `tests/golden` + zipcore | macos-15, windows-2022 |
| Windows 解凍 oracle | 7-Zip `-mcp=932` | windows-2022 |
| 補助: Expand-Archive / tar / Shell.Application | 構造が読めること、bit 11 誤爆検出 | windows-2022（失敗してもジョブは落とさない） |
| Tauri ビルド + E2E 画面 | `apps/winzip-jp/e2e` | macos-15 のみ |
| サンドボックス起動 | ad-hoc codesign + entitlements | macos-15 |

`macos-14` は使わない（2026-11-02 廃止）。`macos-15` は arm64 なので N2/N3 の CI 上の証明になる。

### 5.3 ゴールデンファイル

`tests/golden/` は **入力ディレクトリ** と **期待するファイル名バイト（hex）** をコミットする。期待 ZIP バイナリそのものは Deflate のタイムスタンプで揺れやすいので、コミットするのは次に限る。

- `expected.json`: 各エントリの `name_hex`、`gpbf`、`method`、除外されたパス
- 一部の「構造サンプル」ZIP（bit 11 あり UTF-8、bit 11 なし UTF-8 Finder 風、CP932 典型、混在）は **解凍テストの入力** としてバイナリをコミットする。作成テストの期待出力には使わない

作成テスト: `src/` を `write_zip` → 成果物を parse → `name_raw` と `gpbf` が `expected.json` と一致。

ケース（最低限）:

1. 漢字・かな・全角英数
2. NFD の濁点（`か` + 結合濁点）
3. `.DS_Store` / `__MACOSX` 混入
4. 髙 﨑 絵文字（置換）
5. 名前衝突（置換後）
6. 空フォルダ
7. 深い階層
8. ASCII のみ
9. AES あり（ヘッダ bit 0、extra 0x9901。ペイロードは非決定なので名前とフラグだけ見る）
10. 分割（ピース数と、最後のピースに中央ディレクトリがあること）
11. NEC/IBM 拡張の目視用（①ⅱ㈱）。`encoding_rs` と Windows CP932 の差が出たら変換表で吸収

ゴールデンの expected 更新は専用コミット / PR。レビュー必須。実ユーザーデータは入れない。

### 5.4 Windows ジョブの oracle

```powershell
# tests/oracles/windows-extract.ps1
# 1) raw バイト: Python 3（ランナー標準）
#    zipfile は bit 11 なしを CP437 としてデコードする。
#    CP437 は 0x00-0xFF 全単射なので encode('cp437') で生バイトが戻る。
python tests/oracles/inspect_zip.py out.zip --expect tests/golden/foo/expected.json

# 2) Unicode 名: 7-Zip に CP932 を明示
7z x -y -mcp=932 -o"$out" out.zip
python tests/oracles/compare_extracted.py "$out" tests/golden/foo/expected.json
```

7-Zip はジョブ先頭で `choco install 7zip -y`。

補助（失敗してもジョブは落とさない、レポートのみ）:

- `Expand-Archive` — en-US では名前が CP1252 解釈になる。**bit 11 が落ちていることのスモーク**（日本語が UTF-8 として正しく出たら、誤って bit 11 を立てている）
- `tar -xf`（bsdtar）— 参考
- `Shell.Application.NameSpace` — 同上、ACP 依存

`Expand-Archive` の「日本語が正しく出た」は、この製品では失敗を意味しうる。スクリプトのコメントにその旨を書く。

### 5.5 ワークフロー骨格

`.github/workflows/ci.yml`:

```yaml
name: CI
on:
  pull_request:
  push:
    branches: [main]

jobs:
  zipcore:
    strategy:
      fail-fast: false
      matrix:
        os: [macos-15, windows-2022, ubuntu-24.04]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Test zipcore
        run: cargo test -p zipcore --all-features

  windows-oracle:
    runs-on: windows-2022
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: choco install 7zip -y
      - run: cargo test -p zipcore --test golden -- --ignored

  ui-e2e:
    runs-on: macos-15
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/setup-node@v4
        with: { node-version: 22 }
      - run: pnpm install --frozen-lockfile
        working-directory: apps/winzip-jp
      - run: pnpm run build:debug
        working-directory: apps/winzip-jp
      - run: pnpm run e2e
        working-directory: apps/winzip-jp
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: e2e-screenshots
          path: apps/winzip-jp/e2e/artifacts/
```

---

## 6. 画面検証（人手なし）

画面は「動くはず」ではなく、**macOS ランナーが操作して PNG を残し、ベースラインと比較する**。クラウドエージェントが実装 PR を出したら、同じ CI が赤/緑を返す。

### 6.1 手段

Tauri 公式の **WebdriverIO + `@wdio/tauri-service` + `tauri-plugin-wdio-webdriver`（embedded）**。
macOS の WKWebView には外部 WebDriver が無い。`tauri-driver` は Windows/Linux。提出用 App Store ビルドには webdriver プラグインを入れない（`tauri.e2e.conf.json` で切替）。

```ts
// apps/winzip-jp/e2e/wdio.conf.ts
export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.ts'],
  capabilities: [{ maxInstances: 1, browserName: 'tauri' }],
  services: [['tauri', { appBinaryPath: '../src-tauri/target/debug/winzip-jp', driverProvider: 'embedded' }]],
  framework: 'mocha',
  reporters: ['spec'],
};
```

OS レベルの D&D が再現しにくい場合は、同じ Tauri コマンドを `browser.tauri.execute` で呼び、UI の反応だけを画面で確認する。ロジックの回帰は zipcore、画面は「警告が出た」「切替で一覧が変わった」。

### 6.2 シナリオ

各 spec の末尾で必ず `browser.saveScreenshot('e2e/artifacts/<id>.png')`。

| ID | 操作 | 見て取るもの |
| --- | --- | --- |
| S1 | 起動 | 「作成」が選ばれたドロップゾーン。日本語コピー |
| S2 | フィクスチャフォルダをドロップ（またはコマンド直叩き） | ファイル一覧。`.DS_Store` が無い |
| S3 | 髙.png を含むセット | 不能文字シート。置換「高」が見える |
| S4 | 置換確定 → 保存先をモック → 作成 | 進捗オーバーレイ → 完了 |
| S5 | 解凍モードへ切替 | セグメントが「解凍」 |
| S6 | ゴールデン CP932 ZIP を開く | 一覧が漢字。エンコーディング表示が CP932 |
| S7 | UTF-8 に手動切替 | 一覧が化ける（CP932 ZIP なので）。切替が効いている証明 |
| S8 | 元に戻して展開 | 成果物パスをコマンド経由で assert |
| S9 | AES オプションを開く | 警告文「相手が 7-Zip などを持っていないと開けません（Windows 標準では開けません）」 |
| S9d | パスワード付きで作成 | 確認モーダル。ボタン「このまま作成」「パスワードなしで作成」 |
| S10 | 空の作成（ジャンクだけ） | エラーメッセージ |

### 6.3 ピクセル比較

- ベースライン: `e2e/baselines/<id>.png` をリポジトリにコミット
- CI: `pixelmatch`、コンテンツ領域（ドロップゾーン以下。信号ボタンは切る）、許容 **1%**
- 差があれば `e2e/artifacts/diff-<id>.png` も artifact に上げる
- ベースライン更新は S 番号ごとにレビューする専用 PR

### 6.4 サンドボックス下の画面

通常 E2E は非サンドボックス debug ビルド。追加ジョブ:

```
# 提出用 tauri.appstore.conf.json を直接叩かない。
# signingIdentity と Team ID 付き entitlements のまま ad-hoc すると SIGKILL する。
bash apps/winzip-jp/scripts/sandbox-e2e.sh
```

オープンダイアログは CI で操作しづらいので、アプリ引数 `--e2e-open <fixture>` を debug だけ許可し、そのパスは「ユーザーが選んだ」扱いで `startAccessing` する。`--e2e-open` は App Store ビルドでコンパイルされない（`cfg(e2e)`）。サンドボックスジョブは「コンテナ外のフィクスチャを読めて、ホーム配下の無関係なパスを読むと失敗する」を assert する。

### 6.5 クラウドエージェント向け

UI を変えたら taste-skill に従い、ブラウザまたは画面で確認する（`AGENTS.md`）。CI の受け入れは引き続き artifact の PNG と pixelmatch。

1. 画面をブラウザで操作し、ライト / ダークと作成 / 解凍を見る
2. 該当 S 番号の spec を足す / 通す
3. PR の `ui-e2e` を待つ
4. artifact の PNG を見てコピーとレイアウトを確認
5. ベースライン差分 PR が必要なら別にする

---

## 7. PullFrog による PR 自動レビュー

リポジトリは PullFrog コンソールに **追加済み**。GitHub App のインストールとコンソール側のリポジトリ紐付けは終わっている前提。バージョン管理するのはワークフローとレビュー指示。

参照実装: `ukir-rrrr/my-blog-site/.github/workflows/pullfrog.yml`（`workflow_dispatch` のみ、`actions/checkout@v6`、`pullfrog/pullfrog@v0`、`id-token: write`）。

### 7.1 役割分担（二重レビューをしない）

| 経路 | 用途 |
| --- | --- |
| `.github/workflows/pullfrog.yml` | コンソールのプロンプト、`@pullfrog` メンション。公式の `workflow_dispatch` |
| `.github/workflows/pullfrog-review.yml` | Arena 要件の **PR 自動レビュー**（`pull_request` トリガー） |
| コンソール Automations → Review PRs | **オフ**。オンにすると同じ PR にレビューが二重投稿される |

コンソール自体は残す。オフにするのは「PR を開いたら自動レビュー」だけ。公式の Getting Started では、PR レビューはコンソールが default branch の `pullfrog.yml` を dispatch する。Arena が `pull_request` トリガーを要求するため、レビュー専用ワークフローを別ファイルで持つ。

### 7.2 公式ワークフロー

`.github/workflows/pullfrog.yml`（参照リポジトリに合わせる。Keys は Pullfrog Router なら不要）:

```yaml
# PULLFROG ACTION — DO NOT EDIT EXCEPT WHERE INDICATED
name: Pullfrog
run-name: ${{ inputs.name || github.workflow }}
on:
  workflow_dispatch:
    inputs:
      prompt:
        type: string
        description: Agent prompt
      name:
        type: string
        description: Run name

permissions:
  contents: read

jobs:
  pullfrog:
    runs-on: ubuntu-latest
    permissions:
      id-token: write
      contents: read
    steps:
      - name: Checkout code
        uses: actions/checkout@v6
        with:
          fetch-depth: 1
      - name: Run agent
        uses: pullfrog/pullfrog@v0
        with:
          prompt: ${{ inputs.prompt }}
```

コンソールの「Verify manual installation」対象がこのファイル。

### 7.3 PR レビューワークフロー

`.github/workflows/pullfrog-review.yml`:

```yaml
name: Pullfrog PR Review

on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]

concurrency:
  group: pullfrog-review-${{ github.event.pull_request.number }}
  cancel-in-progress: true

jobs:
  review:
    if: github.event.pull_request.draft == false
    runs-on: ubuntu-latest
    permissions:
      id-token: write
      contents: read
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
          ref: ${{ github.event.pull_request.head.sha }}
      - name: Review pull request
        uses: pullfrog/pullfrog@v0
        with:
          prompt_file: .pullfrog/review.md
          timeout: 20m
          push: disabled
```

Pullfrog の GitHub 操作はワークフローの `GITHUB_TOKEN` を使わない。必要なのは `id-token: write` と checkout 用の `contents: read` だけ。ドラフトは対象外。`ready_for_review` で初回レビュー。

### 7.4 `.pullfrog/review.md`

```markdown
この PR をレビューする。winzip-jp は「Windows で日本語ファイル名が化けない ZIP」が製品の核。

必ず見ること:
- ファイル名バイトが CP932 以外で書かれていないか、EFS bit 11 を立てていないか
- NFD のままエンコードしていないか
- `.DS_Store` / `__MACOSX` 除外が外れていないか
- サンドボックス外パス（$HOME 包括、persisted-scope だけ、startAccessing なし）を足していないか
- テストを期待 Unicode 文字列だけで済ませ、ZIP raw バイトを見ていない変更
- C の圧縮ライブラリ、/usr/bin/zip や ditto の spawn、Rosetta 前提
- パスワードのログ出力、AES をデフォルトオン

フォーマットや命名だけの指摘はしない。欠陥だけインラインで。
日本語 UI の文言漏れ（英語のまま）は欠陥として扱う。
```

### 7.5 ブランチ保護（1.0 前に）

必須チェック: `zipcore`（macos-15 と windows-2022）、`ui-e2e`、`Pullfrog PR Review`。
公式 `pullfrog.yml` の dispatch は default branch に付き PR checks に乗らない。`pullfrog-review.yml` ジョブ自体が PR check になる。

---

## 8. 段階的実装計画（PR 単位）

各 PR はマージ条件を「人が見た」ではなく **CI が証明** する大きさにする。依存は前方向のみ。UI から始めない（画面の緑がコアのバグを隠す）。

画面がまだ無い PR でも、検証方法を明示する。ZIP をまだ書かない段階は「画面なし」と書き、後続の S 番号に接続する。

### PR 0 — 骨格

内容: ワークスペース、`crates/zipcore` 空 lib、Tauri アプリの Hello、`ci.yml` の zipcore ジョブ、`pullfrog.yml` + `pullfrog-review.yml`、`.pullfrog/review.md`、pnpm lockfile。

完了条件:

- `cargo test -p zipcore` が macos-15 / windows-2022 で緑
- 空の Tauri debug が macos-15 でビルドできる
- `pullfrog.yml` が参照リポジトリと同じ `workflow_dispatch` 形
- `pullfrog-review.yml` がこの PR 自身で走ること（コンソールの Review PRs はオフ済み）

画面検証: macos-15 で debug 起動し S1 相当の 1 枚（「作成」ドロップゾーンの骨組み、日本語）を artifact に上げる。pixelmatch はまだしない（ベースライン未確定）。

### PR 1 — エンコーディングだけ

内容: `encode_entry_name` / `detect_encoding` / `decode_name` / 置換表 / ジャンク判定。ZIP をまだ書かない。

完了条件:

- NFD `が` → NFC → CP932 バイトが期待 hex と一致
- 髙 → 高、絵文字 → `_`
- bit 11 あり/なし、Finder 風 UTF-8、CP932 のフィクスチャ配列で `detect_encoding` が表どおり
- ジャンク判定の表テスト

画面検証: なし（CLI / `cargo test`）。後続 S3 / S6 / S7 がこの表の画面側受け入れ。

### PR 2 — ZIP 作成（非暗号・非分割）

内容: PKZIP ヘッダ writer + flate2。`write_zip`。ゴールデン `expected.json`。`inspect_zip.py`。windows-oracle ジョブ。

完了条件:

- 全エントリ `name_raw` と `gpbf & 0x0800 == 0`
- `.DS_Store` / `__MACOSX` が中央ディレクトリに無い
- `inspect_zip.py` が golden を通過
- windows-oracle: 7-Zip `-mcp=932` で NFC 名が一致
- Expand-Archive 補助レポートが「日本語を UTF-8 として正しく展開していない」（bit 11 誤爆の検出）

画面検証: なし。成果 ZIP は後続 S4 / S6 のフィクスチャになる。

### PR 3 — 解凍 + 切替

内容: `zip` crate で list/extract。パストラバーサル拒否。

完了条件:

- コミット済み CP932 ZIP / UTF-8 ZIP / 曖昧 ZIP の判定が表どおり
- override で一覧の `display_name` が変わるテスト
- `../evil` エントリで `extract_zip` が Err
- Windows で作った実物（golden 入力）を macos-15 で展開し中身バイトが一致

画面検証: なし。後続 S5–S8 が画面側受け入れ。

### PR 4 — Tauri コマンドとサンドボックス配線

内容: コマンド、dialog、drop、tmp→rename、entitlements、fs scope。UI は仮でボタンだけ。

完了条件:

- macos-15 で create/list/extract を Rust 統合テスト（ダイアログはパス直指定）
- `tauri.appstore.conf.json` の entitlements が §4.1 と一致するスナップショットテスト
- `$HOME/**` が fs scope に含まれないこと（コンフィグの grep テスト）
- ジョブ開始 2 秒以内に `zip://progress` の `starting` が出るテスト

画面検証: debug ビルドでコマンドをボタンから呼び、進捗テキストが出る 1 枚を artifact に残す（S4 の下書き）。pixelmatch は PR 5。

### PR 5 — UI（作成・解凍・警告）と画面 CI

内容: Create/Extract/Progress、日本語コピー、E2E S1–S8、ベースライン PNG、VoiceOver ラベル。

完了条件:

- `ui-e2e` が macos-15 で緑
- artifact に S1–S8 の PNG
- pixelmatch がコンテンツ領域 1% 以内
- 不能文字シートが S3 で表示される（テキスト assert + 画像）
- 主要コントロールに accessibility label がある（S1 で「作成」「解凍」を VoiceOver 名で assert）

画面検証: §6.2 の S1–S8 そのもの。クラウドエージェントは artifact を見てコピーを確認する。

### PR 6 — AES-256 AE-2

内容: パスワードオプション、zeroize、7-Zip で復号確認。

完了条件:

- パスワードなし ZIP と同じ `name_raw` / bit 11 = 0
- 7-Zip がパスワード付きで展開でき、名前が CP932 解釈で一致
- 空パスワード拒否
- UI S9。警告文のテスト

画面検証: S9。警告文が画像とテキスト assert の両方で固定されていること。

### PR 7 — 分割

内容: `split_bytes`。`.z01` … 最後が `.zip`。

分割の上書きは、新しいセットに含まれない同じ stem の `.zNN` を消す（別アーカイブではなくその ZIP のピース）。非分割の保存は隣の `.zNN` を消さない。読み取りは EOCD の ThisDisk で兄弟を切る。

完了条件:

- 指定サイズを超えるピースが無い（最後を除く）
- 7-Zip が分割アーカイブとして開き、名前が一致
- キャンセルで全ピースを削除

画面検証: 分割オプションを開いた画面 1 枚（「エクスプローラーでは開けません」）。E2E シナリオ ID は S9 の姉妹として `S9b`。

### PR 8 — MAS 近似と提出準備

内容: サンドボックス E2E、アイコン、`ITSAppUsesNonExemptEncryption`、カテゴリ Utility、ストア用スクリーンショット用ダミー。

完了条件:

- ad-hoc 署名サンドボックスビルドがフィクスチャを読み、無関係パスで失敗
- App Store 用 bundle が `macos-15` で `.pkg` まで到達（提出キーは secrets、無ければ `--no-upload`）
- S1 相当をサンドボックス起動でも 1 枚残す

画面検証: §6.4。`--e2e-open` でフィクスチャを開き S1 を撮る。無関係パスは画面ではなくコマンドの Err で assert。

PR 0→5 が「文字化けしない作成・解凍」。6–7 が MVP の残オプション。8 が提出。

---

## 9. リスクと未決事項

### 9.1 リスク

| リスク | 影響 | 扱い |
| --- | --- | --- |
| CP932 列が偶然 valid UTF-8 で、将来 writer を `zip` crate に置き換えた人が bit 11 を立てる | Windows で化ける | writer を自前のままにする。ゴールデンが bit 11 を監視。crate に opt-out が付いたらゴールデンを通して差し替え可 |
| 先方が UTF-8 前提（新しいエクスプローラー + bit 11 期待） | 一部ツールが CP932 を UTF-8 と誤読 | 対象は「日本語 Windows の従来展開」。上級オプションは v1.1 |
| AES ZIP をエクスプローラーが開けない | サポート問い合わせ | デフォルトオフ。警告文を E2E で固定 |
| 分割 ZIP もエクスプローラー非対応 | 同上 | 同上 |
| `encoding_rs::SHIFT_JIS` と Windows CP932 のごく一部の差（NEC/IBM 拡張） | 稀な文字 | golden に ①ⅱ㈱ を含め、差が出たら変換表で吸収 |
| MAS の encryption 質問 | 提出遅延 | PR 8 で確定 |
| Webdriver embedded のフレーク | 画面 CI が赤い | 比較領域をコンテンツだけにする。コマンド直叩きテストを併設 |
| 商標「WinZip」 | リジェクト | 表示名は「文字化けしないZIP」。Bundle ID は `jp.riku.winzip` のまま |
| 競合 WinZipper の M4 不具合の再現条件が不明 | 差別化の検証が弱い | 純粋 Rust + 起動直後に progress。外部バイナリを呼ばない |
| `tauri-plugin-scoped-bookmarks` を後から足すときの権限リーク | ホーム全体 grant | v1.1 で出力先ディレクトリ単位に限り、forget UI を必須にする |

### 9.2 未決（実装 PR の前に決めるもの）

1. **ストア表示名**（決定）。ユーザー向け表示名は「文字化けしないZIP」。Bundle ID は `jp.riku.winzip`、リポジトリ / crate 名は `winzip-jp` のまま
2. **表示価格**。レンジは ¥980〜1,500。推奨 ¥1,280
3. **AES を 1.0 に残すか**。残すなら警告必須。問い合わせコストを嫌うなら PR 6 を 1.1 に送る（要件上は MVP なので 1.0 残しを既定とする）
4. **分割サイズのプリセット**。メール 10 MB を既定候補。会社の上限は 5 / 7 / 25 もある
5. **`ITSAppUsesNonExemptEncryption`**
6. **不能文字をユーザーが拒否できるか**。既定は「確認して作る」（拒否ボタンあり）
7. **解凍時の `.DS_Store` をスキップするか**。作成時は除外、解凍時は「ジャンク表示オフ」既定でスキップする案
8. **Apple Developer Team ID**（決定）。個人アカウント `CHSRTG92SZ`。Bundle ID は `jp.riku.winzip`。提出用 `signingIdentity` は `Apple Distribution`（実体はキーチェーンの `Apple Distribution: <氏名> (CHSRTG92SZ)`。`apps/winzip-jp/scripts/appstore-pkg.sh` が Team ID で解決する）。カテゴリは Utility。手順は `docs/release/submit-guide.md`
9. 圧縮レベルデフォルト（6 vs 最大）。ベンチ後

### 9.3 意図的にやらないこと（再掲）

- UTF-8 併記 extra field
- ZipCrypto
- Intel バイナリ
- 作成側 ZIP64
- ホームフォルダへの永続アクセス（MVP）
- PullFrog コンソールの自動 Review と GHA `pullfrog-review.yml` の併用

---

## 付録 A. 作成ヘッダの固定値

ローカルファイルヘッダ（1 エントリ）:

- signature `50 4b 03 04`
- version needed `14 00`（2.0、Deflate）
- gpbf: bit 11 = 0。AES 時は bit 0 = 1
- method `08 00`（ファイル）/ `00 00`（ディレクトリ）
- filename = CP932 バイト、長さは u16
- extra: 非 AES なら空。AES なら 0x9901 のみ

中央ディレクトリの filename はローカルと **同一バイト**。ズレはバグ。ホスト OS = 0（MS-DOS）。

## 付録 B. UI コピー（1.0）

- 「文字化けしないZIP」（ウィンドウ／ツールバー／Finder の表示名）
- 「ファイルをドロップして ZIP を作成」
- 「文字化けした ZIP をドロップして解凍」
- 「Windows で開ける形式（CP932）で保存します」
- 「次の名前は Windows で使えない文字を含むため、置き換えて保存します」
- 「相手が 7-Zip などを持っていないと開けません（Windows 標準では開けません）」
- 「相手が Windows 標準で開くなら、パスワードはオフにしてください」
- 「分割した ZIP は Windows 標準では開けません。7-Zip などが必要です。」
- 「相手が Windows 標準で開くなら、分割はオフにしてください」
- 「このまま作成」
- 「パスワードなしで作成」
- 「次回から表示しない」
- 「受け取る人に 7-Zip が必要と伝えてください」
- 「分割した ZIP は受け取る人に 7-Zip が必要です」
- 「パスワード」（入力プレースホルダー）
- 「表示」（パスワードの表示切替。読み上げ名は「パスワードを表示」。状態は aria-pressed）
- 「ZIP にパスワードを付けます。エクスプローラーでは開けません。」
- 「指定サイズで分けます。エクスプローラーでは開けません。」
- 「Windows で開ける ZIP を作ります。」
- 「文字化けした ZIP を正しく展開します。」
- 「使い方」（ボタン）。読み上げ名は「文字化けしないZIP の使い方」
- 「ファイルをドロップする」
- 「必要ならパスワードや分割を付ける」
- 「保存先を選ぶ」
- 「解凍タブでは、Windows から来た文字化け ZIP を直せます。」
- 「とじる」

英語のフォールバック文字列をコンポーネントに残さない。
