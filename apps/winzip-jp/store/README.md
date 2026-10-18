# ストア提出用の置き画像（旧 1280×800）

提出用の本文と 2880×1800 スクリーンショットの正は [`docs/store/listing.md`](../../../docs/store/listing.md) です。

ここにある 1280×800 は、古い枠を埋めるための置き画像です。中身は E2E の S1 / S5 ベースラインを拡大したもので、提出用の最終画ではありません。

- `screenshots/s1-1280x800.png` — 作成ドロップゾーン
- `screenshots/s5-1280x800.png` — 解凍モード

実機（サンドボックス起動）の S1 は CI artifact `s1-sandbox.png` を正とします。
輸出コンプライアンスは App Store Connect で答える。Info.plist に `ITSAppUsesNonExemptEncryption=true` は置かない。
このリポジトリの CI は `.pkg` まで作り、App Store へはアップロードしません。
