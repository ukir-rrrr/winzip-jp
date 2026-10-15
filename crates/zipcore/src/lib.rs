//! Windows 向け ZIP のファイル名とフラグを自前で扱うコア。
//!
//! モジュール境界は `encoding` / `write` / `read` / `split`。
//! `split` は作成の出力ストリームだけを差し替える。

pub mod encoding;
pub mod read;
pub mod split;
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
pub use split::{discover_volumes, join_volumes, join_volumes_to};
pub use write::{
    inspect_zip_headers, plan_write, write_zip, AesMode, InputItem, InspectedEntry, Plan,
    PlannedEntry, WriteReport, WriteRequest,
};
