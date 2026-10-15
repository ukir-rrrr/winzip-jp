//! Windows 向け ZIP のファイル名とフラグを自前で扱うコア。
//!
//! モジュール境界は `encoding` / `write` / `read` / `split`。
//! PR 6 は `encoding` / `write` / `read`（AES-256 AE-2 任意、非分割）。

pub mod encoding;
pub mod read;
pub mod write;

mod aeswrite;
mod error;
mod fsutil;
mod job;
mod pathpolicy;
mod pkzip;

pub use encoding::{
    decode_name, detect_encoding, encode_entry_name, is_junk, Detection, EncodedName, NameEncoding,
    Unmappable, UnmappableKind, COMPAT_IDEOGRAPH_REPLACEMENTS, EFS_UTF8_BIT, UNICODE_PATH_EXTRA_ID,
};
pub use error::ZipcoreError;
pub use fsutil::{
    absolute, atomic_replace, normalize_lexically, path_eq, resolve_existing, resolve_path,
    stays_under,
};
pub use job::{Progress, ProgressPhase, SecretString};
pub use read::{
    extract_zip, list_zip, ExtractReport, ExtractRequest, ListReport, ListRequest, ListedEntry,
};
pub use write::{
    inspect_zip_headers, plan_write, write_zip, AesMode, InputItem, InspectedEntry, Plan,
    PlannedEntry, WriteReport, WriteRequest,
};
