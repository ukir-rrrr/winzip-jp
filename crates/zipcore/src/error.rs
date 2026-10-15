//! zipcore のエラー型。メッセージは UI に出す前提で日本語。

use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::encoding::Unmappable;

#[derive(Debug)]
pub enum ZipcoreError {
    Io(io::Error),
    /// ジャンク除外後に追加できる入力が無い。
    EmptyInputs,
    Cancelled,
    AesNotImplemented,
    SplitNotImplemented,
    Zip64Required {
        reason: String,
    },
    UnconfirmedReplacements {
        issues: Vec<Unmappable>,
    },
    NotADirectory {
        path: PathBuf,
    },
    NotAFile {
        path: PathBuf,
    },
    NameTooLong {
        path: String,
    },
    InputMissing {
        path: PathBuf,
    },
    UnsupportedType {
        path: PathBuf,
    },
    CollisionLimit {
        path: String,
    },
    DestOverlapsInput {
        dest: PathBuf,
    },
    Corrupt(String),
    /// `..` / 絶対パス / ドライブレター / バックスラッシュ / シンボリックリンクなど。
    PathTraversal {
        entry: String,
    },
    OverwriteRequired {
        paths: Vec<String>,
    },
    Encrypted {
        entry: String,
    },
    UnsupportedEntry {
        entry: String,
        reason: String,
    },
    LimitsExceeded {
        reason: String,
    },
}

impl fmt::Display for ZipcoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "入出力エラー: {e}"),
            Self::EmptyInputs => write!(f, "追加できるファイルがありません"),
            Self::Cancelled => write!(f, "キャンセルされました"),
            Self::AesNotImplemented => {
                write!(f, "パスワード付き ZIP はこのバージョンではまだ作れません")
            }
            Self::SplitNotImplemented => {
                write!(f, "分割 ZIP はこのバージョンではまだ作れません")
            }
            Self::Zip64Required { reason } => {
                write!(f, "ZIP64 が必要なため作成できません: {reason}")
            }
            Self::UnconfirmedReplacements { .. } => {
                write!(f, "Windows で使えない文字の置き換えを確定してください")
            }
            Self::NotADirectory { path } => {
                write!(f, "フォルダではありません: {}", path.display())
            }
            Self::NotAFile { path } => write!(f, "ファイルではありません: {}", path.display()),
            Self::NameTooLong { path } => write!(f, "エントリ名が長すぎます: {path}"),
            Self::InputMissing { path } => {
                write!(f, "入力が見つかりません: {}", path.display())
            }
            Self::UnsupportedType { path } => {
                write!(f, "ファイルでもフォルダでもありません: {}", path.display())
            }
            Self::CollisionLimit { path } => {
                write!(f, "同名エントリの連番が上限を超えました: {path}")
            }
            Self::DestOverlapsInput { dest } => {
                write!(f, "出力先が入力フォルダの中にあります: {}", dest.display())
            }
            Self::Corrupt(msg) => write!(f, "ZIP が壊れています: {msg}"),
            Self::PathTraversal { entry } => {
                write!(f, "ZIP 内のパスが展開先の外に出ます: {entry}")
            }
            Self::OverwriteRequired { paths } => {
                write!(
                    f,
                    "次のファイルは既にあります。上書きを許可してください: {}",
                    paths.join(", ")
                )
            }
            Self::Encrypted { entry } => {
                write!(f, "パスワード付きのエントリがあります: {entry}")
            }
            Self::UnsupportedEntry { entry, reason } => {
                write!(f, "展開できないエントリです ({reason}): {entry}")
            }
            Self::LimitsExceeded { reason } => write!(f, "ZIP が大きすぎます: {reason}"),
        }
    }
}

impl std::error::Error for ZipcoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ZipcoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<zip::result::ZipError> for ZipcoreError {
    fn from(value: zip::result::ZipError) -> Self {
        match value {
            zip::result::ZipError::Io(e) => Self::Io(e),
            zip::result::ZipError::InvalidArchive(msg) => Self::Corrupt(msg.into_owned()),
            zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED) => {
                Self::Encrypted {
                    entry: String::new(),
                }
            }
            zip::result::ZipError::UnsupportedArchive(msg) => Self::UnsupportedEntry {
                entry: String::new(),
                reason: msg.to_string(),
            },
            zip::result::ZipError::FileNotFound => Self::Corrupt("エントリが見つかりません".into()),
            zip::result::ZipError::InvalidPassword => Self::Encrypted {
                entry: String::new(),
            },
            zip::result::ZipError::CompressionMethodNotSupported(m) => Self::UnsupportedEntry {
                entry: String::new(),
                reason: format!("圧縮メソッド {m}"),
            },
            other => Self::Corrupt(other.to_string()),
        }
    }
}
