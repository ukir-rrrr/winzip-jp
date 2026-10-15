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

/// 存在するなら canonicalize。未作成なら親を canonicalize してファイル名を足す。
///
/// macOS の `/var` → `/private/var` のように、temp の実パスと `temp_dir()` の
/// 表記が違うときに、grant と判定を同じキーへ揃える。
pub fn resolve_path(path: &Path) -> PathBuf {
    let abs = normalize_lexically(&absolute(path));
    if let Ok(c) = fs::canonicalize(&abs) {
        return c;
    }
    if let Some(parent) = abs.parent() {
        if let Ok(pc) = fs::canonicalize(parent) {
            if let Some(name) = abs.file_name() {
                return pc.join(name);
            }
        }
    }
    abs
}

pub fn resolve_existing(path: &Path) -> PathBuf {
    let abs = normalize_lexically(&absolute(path));
    fs::canonicalize(&abs).unwrap_or(abs)
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

fn copy_replace(from: &Path, to: &Path) -> io::Result<()> {
    if let Err(e) = fs::copy(from, to) {
        let _ = fs::remove_file(to);
        return Err(e);
    }
    let _ = fs::remove_file(from);
    Ok(())
}

fn place_after_backup(from: &Path, to: &Path, backup: PathBuf) -> io::Result<Option<PathBuf>> {
    match fs::rename(from, to).or_else(|_| copy_replace(from, to)) {
        Ok(()) => Ok(Some(backup)),
        Err(e) => {
            let _ = fs::rename(&backup, to);
            Err(e)
        }
    }
}

/// `from` を `to` に置く。Unix では既存 `to` も `rename` で原子置換する。
/// Windows で上書き rename できないときだけ sidecar に退避し、そのパスを返す。
pub fn atomic_replace(from: &Path, to: &Path) -> io::Result<Option<PathBuf>> {
    if fs::rename(from, to).is_ok() {
        return Ok(None);
    }
    if !to.exists() {
        copy_replace(from, to)?;
        return Ok(None);
    }
    let backup = unique_sidecar(to, "bak");
    fs::rename(to, &backup)?;
    place_after_backup(from, to, backup)
}

/// 既存 `to` を必ず sidecar に退避してから置く。解凍のキャンセル巻き戻し用。
pub fn replace_keeping_backup(from: &Path, to: &Path) -> io::Result<Option<PathBuf>> {
    if !to.exists() {
        if fs::rename(from, to).is_ok() {
            return Ok(None);
        }
        copy_replace(from, to)?;
        return Ok(None);
    }
    let backup = unique_sidecar(to, "bak");
    fs::rename(to, &backup)?;
    place_after_backup(from, to, backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_pair(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "winzip-jp-fsutil-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let from = root.join("from.bin");
        let to = root.join("to.bin");
        (root, from, to)
    }

    #[test]
    fn atomic_replace_overwrites_without_leaving_backup() {
        let (root, from, to) = temp_pair("atomic");
        fs::write(&from, b"new").unwrap();
        fs::write(&to, b"old").unwrap();
        let backup = atomic_replace(&from, &to).unwrap();
        assert_eq!(fs::read(&to).unwrap(), b"new");
        if let Some(backup) = backup {
            let _ = fs::remove_file(backup);
        }
        let mut leftovers: Vec<String> = fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with('.'))
            .collect();
        leftovers.sort();
        assert_eq!(leftovers, vec!["to.bin".to_string()]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_path_follows_symlink_parent() {
        let (root, _, _) = temp_pair("resolve");
        let real = root.join("real");
        fs::create_dir_all(&real).unwrap();
        let alias = root.join("alias");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real, &alias).unwrap();
            let pending = alias.join("out.zip");
            let resolved = resolve_path(&pending);
            let canon_real = fs::canonicalize(&real).unwrap();
            assert_eq!(resolved, canon_real.join("out.zip"));
            assert_ne!(pending, resolved);
        }
        #[cfg(not(unix))]
        {
            let pending = real.join("out.zip");
            let resolved = resolve_path(&pending);
            let canon_real = fs::canonicalize(&real).unwrap();
            assert_eq!(resolved, canon_real.join("out.zip"));
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn replace_keeping_backup_preserves_old_bytes() {
        let (root, from, to) = temp_pair("backup");
        fs::write(&from, b"new").unwrap();
        fs::write(&to, b"old").unwrap();
        let backup = replace_keeping_backup(&from, &to).unwrap().expect("backup");
        assert_eq!(fs::read(&to).unwrap(), b"new");
        assert_eq!(fs::read(&backup).unwrap(), b"old");
        let _ = fs::remove_file(backup);
        let _ = fs::remove_dir_all(&root);
    }
}
