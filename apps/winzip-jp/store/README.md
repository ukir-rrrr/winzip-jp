# ストア提出用のダミー

Mac App Store のスクリーンショット枠（1280×800）を埋めるための置き画像です。
中身は E2E の S1 / S5 ベースラインを拡大したもので、提出用の最終画ではありません。

- `screenshots/s1-1280x800.png` — 作成ドロップゾーン
- `screenshots/s5-1280x800.png` — 解凍モード

実機（サンドボックス起動）の S1 は CI artifact `s1-sandbox.png` を正とします。
AES-256 があるため `ITSAppUsesNonExemptEncryption` は `true`（輸出コンプライアンスの回答は提出時）。
このリポジトリの CI は `.pkg` まで作り、App Store へはアップロードしません。
