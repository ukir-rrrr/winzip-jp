//! 作成 / 解凍で共有するパス比較と原子置換。

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

pub fn strip_verbatim(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix(r"UNC\") {
            PathBuf::from(format!(r"\\{unc}"))
        } else {
            PathBuf::from(rest.to_string())
        }
    } else {
        path.to_path_buf()
    }
}

pub fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

pub fn path_eq(a: &Path, b: &Path) -> bool {
    let a = strip_verbatim(&normalize_lexically(a));
    let b = strip_verbatim(&normalize_lexically(b));
    a == b || a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

/// `child` が `parent` 自身、またはその配下か。
pub fn stays_under(child: &Path, parent: &Path) -> bool {
    let child = strip_verbatim(&normalize_lexically(child));
    let parent = strip_verbatim(&normalize_lexically(parent));
    if child.starts_with(&parent) {
        return true;
    }
    let child_s = child.to_string_lossy().to_lowercase();
    let parent_s = parent.to_string_lossy().to_lowercase();
    if child_s == parent_s {
        return true;
    }
    child_s.starts_with(&format!("{parent_s}/")) || child_s.starts_with(&format!("{parent_s}\\"))
}

pub fn unique_sidecar(path: &Path, kind: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{kind}.{}.{}", std::process::id(), n));
    PathBuf::from(name)
}

/// `from` を `to` に置く。`to` が既にあれば一意な sidecar に退避し、そのパスを返す。
/// 成功時に sidecar を消すかは呼び出し側（作成は消す、解凍はジョブ成功まで残す）。
pub fn atomic_replace(from: &Path, to: &Path) -> io::Result<Option<PathBuf>> {
    if !to.exists() {
        if fs::rename(from, to).is_ok() {
            return Ok(None);
        }
        if let Err(e) = fs::copy(from, to) {
            let _ = fs::remove_file(to);
            return Err(e);
        }
        let _ = fs::remove_file(from);
        return Ok(None);
    }
    let backup = unique_sidecar(to, "bak");
    fs::rename(to, &backup)?;
    let placed = fs::rename(from, to).or_else(|_| {
        fs::copy(from, to)?;
        fs::remove_file(from)?;
        Ok(())
    });
    match placed {
        Ok(()) => Ok(Some(backup)),
        Err(e) => {
            let _ = fs::rename(&backup, to);
            Err(e)
        }
    }
}
