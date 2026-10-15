//! Windows 向け ZIP のファイル名とフラグを自前で扱うコア。
//!
//! モジュール境界は `encoding` / `write` / `read` / `split`。
//! PR 2 は `encoding` と `write`（非暗号・非分割）。

pub mod encoding;
pub mod write;

mod error;
mod pkzip;

pub use encoding::{
    decode_name, detect_encoding, encode_entry_name, is_junk, Detection, EncodedName, NameEncoding,
    Unmappable, UnmappableKind, COMPAT_IDEOGRAPH_REPLACEMENTS, EFS_UTF8_BIT, UNICODE_PATH_EXTRA_ID,
};
pub use error::ZipcoreError;
pub use write::{
    inspect_zip_headers, plan_write, write_zip, AesMode, InputItem, InspectedEntry, Plan,
    PlannedEntry, Progress, ProgressPhase, SecretString, WriteReport, WriteRequest,
};
