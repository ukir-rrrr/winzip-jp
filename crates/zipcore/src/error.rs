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
            Self::Io(e) => write!(f, "入出力エラー: {}", io_kind_ja(e.kind())),
            Self::EmptyInputs => write!(f, "追加できるファイルがありません"),
            Self::Cancelled => write!(f, "キャンセルされました"),
            Self::AesNotImplemented => {
                write!(f, "パスワード付き ZIP はこのバージョンではまだ扱えません")
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

fn io_kind_ja(kind: io::ErrorKind) -> &'static str {
    use io::ErrorKind::*;
    match kind {
        NotFound => "見つかりません",
        PermissionDenied => "権限がありません",
        AlreadyExists => "既にあります",
        InvalidInput | InvalidData => "データが不正です",
        UnexpectedEof => "ファイルが途中で終わりました",
        WriteZero => "書き込めませんでした",
        Interrupted => "中断されました",
        Unsupported => "この操作は扱えません",
        OutOfMemory => "メモリが足りません",
        _ => "読み書きに失敗しました",
    }
}

impl From<zip::result::ZipError> for ZipcoreError {
    fn from(value: zip::result::ZipError) -> Self {
        match value {
            zip::result::ZipError::Io(e) => Self::Io(e),
            zip::result::ZipError::InvalidArchive(_) => {
                Self::Corrupt("アーカイブが壊れています".into())
            }
            zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED) => {
                Self::Encrypted {
                    entry: String::new(),
                }
            }
            zip::result::ZipError::UnsupportedArchive(_) => Self::UnsupportedEntry {
                entry: String::new(),
                reason: "この形式は扱えません".into(),
            },
            zip::result::ZipError::FileNotFound => Self::Corrupt("エントリが見つかりません".into()),
            zip::result::ZipError::InvalidPassword => Self::Encrypted {
                entry: String::new(),
            },
            zip::result::ZipError::CompressionMethodNotSupported(_) => Self::UnsupportedEntry {
                entry: String::new(),
                reason: "非対応の圧縮方式です".into(),
            },
            _ => Self::Corrupt("ZIP を読めません".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_crate_errors_map_to_japanese() {
        let invalid = ZipcoreError::from(zip::result::ZipError::InvalidArchive(
            "Invalid digital signature file".into(),
        ));
        let text = invalid.to_string();
        assert!(text.contains("壊れ"), "{text}");
        assert!(!text.contains("Invalid"), "{text}");

        let unsupported = ZipcoreError::from(zip::result::ZipError::UnsupportedArchive(
            "The compression method is not supported",
        ));
        let text = unsupported.to_string();
        assert!(text.contains("扱えません"), "{text}");
        assert!(!text.contains("compression"), "{text}");

        let other = ZipcoreError::from(zip::result::ZipError::FileNotFound);
        assert_eq!(
            other.to_string(),
            "ZIP が壊れています: エントリが見つかりません"
        );

        let io = ZipcoreError::from(io::Error::new(io::ErrorKind::NotFound, "No such file"));
        let text = io.to_string();
        assert!(text.contains("見つかりません"), "{text}");
        assert!(!text.contains("No such"), "{text}");
        assert!(!text.contains("os error"), "{text}");
    }
}
