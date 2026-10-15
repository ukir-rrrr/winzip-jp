//! セッション内のユーザー選択パスと、コンテナ一時ファイルの公開。
//!
//! アクセスが開く経路はダイアログとドロップだけ。永続 grant は持たない。

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use zipcore::{absolute, atomic_replace, path_eq, stays_under};

const TMP_PREFIX: &str = "winzip-jp-";

#[cfg(target_os = "macos")]
mod scoped {
    use std::ffi::c_void;
    use std::path::Path;

    #[repr(C)]
    struct __CFURL {
        _private: [u8; 0],
    }

    type CFURLRef = *mut __CFURL;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: *const c_void,
            buffer: *const u8,
            buf_len: isize,
            is_directory: u8,
        ) -> CFURLRef;
        fn CFURLStartAccessingSecurityScopedResource(url: CFURLRef) -> u8;
        fn CFURLStopAccessingSecurityScopedResource(url: CFURLRef);
        fn CFRelease(cf: *const c_void);
    }

    pub struct ScopedUrl {
        url: CFURLRef,
    }

    impl ScopedUrl {
        pub fn start(path: &Path, is_dir: bool) -> Option<Self> {
            use std::os::unix::ffi::OsStrExt;
            let bytes = path.as_os_str().as_bytes();
            if bytes.is_empty() {
                return None;
            }
            unsafe {
                let url = CFURLCreateFromFileSystemRepresentation(
                    std::ptr::null(),
                    bytes.as_ptr(),
                    bytes.len() as isize,
                    u8::from(is_dir),
                );
                if url.is_null() {
                    return None;
                }
                let _started = CFURLStartAccessingSecurityScopedResource(url);
                Some(Self { url })
            }
        }
    }

    impl Drop for ScopedUrl {
        fn drop(&mut self) {
            unsafe {
                CFURLStopAccessingSecurityScopedResource(self.url);
                CFRelease(self.url.cast());
            }
        }
    }

    // CFURL は生成後に共有参照するだけ。
    unsafe impl Send for ScopedUrl {}
}

struct Grant {
    path: PathBuf,
    #[cfg(target_os = "macos")]
    _access: Option<scoped::ScopedUrl>,
}

#[derive(Default)]
pub struct Sandbox {
    grants: Vec<Grant>,
}

impl Sandbox {
    pub fn new() -> Self {
        Self { grants: Vec::new() }
    }

    /// ダイアログ / ドロップで渡ったパスをセッション許可に足す。
    pub fn grant(&mut self, path: &Path) {
        let abs = absolute(path);
        if self.grants.iter().any(|g| path_eq(&g.path, &abs)) {
            return;
        }
        let is_dir = abs.is_dir();
        #[cfg(target_os = "macos")]
        let access = scoped::ScopedUrl::start(&abs, is_dir);
        self.grants.push(Grant {
            path: abs,
            #[cfg(target_os = "macos")]
            _access: access,
        });
        let _ = is_dir;
    }

    pub fn grant_many<P: AsRef<Path>>(&mut self, paths: impl IntoIterator<Item = P>) {
        for path in paths {
            self.grant(path.as_ref());
        }
    }

    pub fn allows(&self, path: &Path) -> bool {
        let abs = absolute(path);
        if is_container_tmp(&abs) {
            return true;
        }
        if self.covers(&abs) {
            return true;
        }
        abs.parent().is_some_and(|parent| self.covers(parent))
    }

    fn covers(&self, path: &Path) -> bool {
        self.grants
            .iter()
            .any(|g| path_eq(path, &g.path) || stays_under(path, &g.path))
    }

    pub fn granted_roots(&self) -> Vec<PathBuf> {
        self.grants.iter().map(|g| g.path.clone()).collect()
    }

    /// 許可パスだけを複製する。security-scoped の寿命は元の `Sandbox` が持つ。
    pub fn snapshot_paths(&self) -> Self {
        Self {
            grants: self
                .grants
                .iter()
                .map(|g| Grant {
                    path: g.path.clone(),
                    #[cfg(target_os = "macos")]
                    _access: None,
                })
                .collect(),
        }
    }
}

pub fn container_tmp_zip() -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "{TMP_PREFIX}{}-{}-{n}.zip",
        std::process::id(),
        nanos
    ))
}

pub fn is_container_tmp(path: &Path) -> bool {
    let tmp = std::env::temp_dir();
    let abs = absolute(path);
    if !stays_under(&abs, &tmp) {
        return false;
    }
    let name = abs.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    name.starts_with(TMP_PREFIX) || name.starts_with(".winzip-jp-tmp-")
}

/// コンテナ tmp を fsync してからユーザー先へ rename（別ボリュームは copy+remove）。
pub fn publish_tmp(tmp: &Path, dest: &Path) -> io::Result<()> {
    if tmp.exists() {
        let file = File::open(tmp)?;
        file.sync_all()?;
        drop(file);
    }
    match atomic_replace(tmp, dest) {
        Ok(Some(backup)) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(tmp);
            Err(e)
        }
    }
}

pub fn discard_tmp(tmp: &Path) {
    let _ = fs::remove_file(tmp);
    let mut partial = tmp.as_os_str().to_os_string();
    partial.push(".partial");
    let _ = fs::remove_file(partial);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_prefix_is_allowed_without_grant() {
        let sandbox = Sandbox::new();
        let tmp = container_tmp_zip();
        assert!(sandbox.allows(&tmp));
        assert!(is_container_tmp(&tmp));
    }

    #[test]
    fn ungranted_workspace_path_is_denied() {
        let sandbox = Sandbox::new();
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(!sandbox.allows(&outside));
    }
}
