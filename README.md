# winzip-jp

日本語版 Windows とやり取りする会社員向けの、Mac 専用買い切りアプリです。
作る ZIP は日本語ファイル名が Windows エクスプローラーで化けず、受け取った文字化け ZIP は正しく解凍できます。

設計は [`docs/design.md`](docs/design.md) を正とします。

## 配置

```
crates/zipcore/          # エンコーディング・作成・解凍（PR 1 以降で実装）
apps/winzip-jp/          # Tauri v2 + React + TypeScript
```

パッケージマネージャは pnpm です。ロックファイルはリポジトリ直下の 1 本です。

## 開発

```bash
pnpm install
cargo test -p zipcore
pnpm --filter winzip-jp build
pnpm --filter winzip-jp e2e          # S1 相当のスクリーンショット
pnpm --filter winzip-jp tauri dev    # macOS でアプリを起動
```
