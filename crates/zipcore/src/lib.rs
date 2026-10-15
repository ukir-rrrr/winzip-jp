//! Windows 向け ZIP のファイル名とフラグを自前で扱うコア。
//!
//! モジュール境界は `encoding` / `write` / `read` / `split`。
//! PR 1 では `encoding` のみ。ZIP の読み書きはまだ持たない。

pub mod encoding;

pub use encoding::{
    decode_name, detect_encoding, encode_entry_name, is_junk, Detection, EncodedName, NameEncoding,
    Unmappable, UnmappableKind, COMPAT_IDEOGRAPH_REPLACEMENTS, EFS_UTF8_BIT, UNICODE_PATH_EXTRA_ID,
};
