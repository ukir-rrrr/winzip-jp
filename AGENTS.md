# エージェント向けメモ

このリポジトリの設計の正は [`docs/design.md`](docs/design.md) です。

## UI

UI を変更するときは必ず taste-skill に従い、ブラウザまたは画面で確認する。

taste-skill の導入先:

- `.agents/skills/` — `npx skills add Leonxlnx/taste-skill` の正本。他エージェント共通
- `.cursor/skills/` — Cursor が読む場所。正本へのシンボリックリンク

本製品は Finder らしいデスクトップアプリであり、マーケティング用ランディングページではない。
`design-taste-frontend` のランディング向け手順（方向抽選、写真フロア、8 要素モーション）は適用しない。
使うのはハウスルール（スロップ禁止、実画面確認、`lint.mjs`）と、`minimalist-ui` の抑制である。
ルートに `data-ui` を付け、本物のアプリ枠として扱う。テーマはシステムフォント、ダークモード追従、日本語のみ。
