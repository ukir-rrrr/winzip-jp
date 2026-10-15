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
