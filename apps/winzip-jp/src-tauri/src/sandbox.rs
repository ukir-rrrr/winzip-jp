//! セッション内のユーザー選択パスと、コンテナ一時ファイルの公開。
//!
//! アクセスが開く経路はダイアログとドロップだけ。永続 grant は持たない。

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use zipcore::{absolute, atomic_replace, path_eq, resolve_path, stays_under};

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
    /// フォルダ選択 / フォルダドロップだけ true。保存先ファイルは false。
    recursive: bool,
    #[cfg(target_os = "macos")]
    _access: Option<scoped::ScopedUrl>,
}

#[derive(Default)]
pub struct Sandbox {
    grants: Vec<Grant>,
    #[cfg(target_os = "macos")]
    kernel_holds: Vec<scoped::ScopedUrl>,
}

impl Sandbox {
    pub fn new() -> Self {
        Self::default()
    }

    /// ダイアログ / ドロップで選んだ入力。ディレクトリなら配下も許可。
    pub fn grant(&mut self, path: &Path) {
        let abs = absolute(path);
        let stored = resolve_path(&abs);
        let recursive = stored.is_dir() || abs.is_dir();
        self.insert(abs, stored, recursive);
    }

    pub fn grant_many<P: AsRef<Path>>(&mut self, paths: impl IntoIterator<Item = P>) {
        for path in paths {
            self.grant(path.as_ref());
        }
    }

    /// 保存先 ZIP。ファイルだけ許可し、親フォルダはカーネル許可のみ。
    pub fn grant_save_dest(&mut self, path: &Path) {
        let abs = absolute(path);
        let stored = resolve_path(&abs);
        self.insert(abs.clone(), stored, false);
        if let Some(parent) = abs.parent() {
            self.hold_kernel(parent, true);
        }
    }

    fn insert(&mut self, original: PathBuf, stored: PathBuf, recursive: bool) {
        if let Some(existing) = self.grants.iter_mut().find(|g| path_eq(&g.path, &stored)) {
            existing.recursive |= recursive;
            return;
        }
        let is_dir = stored.is_dir() || original.is_dir();
        // startAccessing はダイアログ/ドロップの元パス。比較キーは canonicalize 後。
        #[cfg(target_os = "macos")]
        let access = scoped::ScopedUrl::start(&original, is_dir);
        self.grants.push(Grant {
            path: stored,
            recursive,
            #[cfg(target_os = "macos")]
            _access: access,
        });
        let _ = (original, is_dir);
    }

    fn hold_kernel(&mut self, path: &Path, is_dir: bool) {
        #[cfg(target_os = "macos")]
        if let Some(url) = scoped::ScopedUrl::start(path, is_dir) {
            self.kernel_holds.push(url);
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (path, is_dir);
        }
    }

    pub fn allows(&self, path: &Path) -> bool {
        self.covers(&resolve_path(path))
    }

    fn covers(&self, path: &Path) -> bool {
        self.grants.iter().any(|g| {
            if path_eq(path, &g.path) {
                return true;
            }
            g.recursive && stays_under(path, &g.path)
        })
    }

    /// コマンド引数を許可ルート配下へ正規化する。
    pub fn resolve(&self, path: &Path) -> Result<PathBuf, String> {
        let raw = path.to_string_lossy();
        if raw.is_empty() || raw.contains('\0') {
            return Err("パスが不正です".into());
        }
        let resolved = resolve_path(path);
        if !self.covers(&resolved) {
            return Err(format!(
                "この場所にはアクセスできません: {}",
                resolved.display()
            ));
        }
        Ok(resolved)
    }

    /// 許可パスだけを複製する。security-scoped の寿命は元の `Sandbox` が持つ。
    /// スナップショットは判定用で、`startAccessing` の保持はしない。
    pub fn snapshot_paths(&self) -> Self {
        Self {
            grants: self
                .grants
                .iter()
                .map(|g| Grant {
                    path: g.path.clone(),
                    recursive: g.recursive,
                    #[cfg(target_os = "macos")]
                    _access: None,
                })
                .collect(),
            #[cfg(target_os = "macos")]
            kernel_holds: Vec::new(),
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
    let tmp = resolve_path(&std::env::temp_dir());
    let abs = resolve_path(path);
    if !stays_under(&abs, &tmp) {
        return false;
    }
    let name = abs.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    name.starts_with(TMP_PREFIX) || name.starts_with(".winzip-jp-tmp-")
}

/// コンテナ tmp を fsync してからユーザー先へ rename（別ボリュームは copy+remove）。
pub fn publish_tmp(tmp: &Path, dest: &Path) -> io::Result<()> {
    if !is_container_tmp(tmp) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "コンテナ一時ファイルではありません",
        ));
    }
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
    fn container_tmp_name_is_recognized() {
        let tmp = container_tmp_zip();
        assert!(is_container_tmp(&tmp));
        assert!(!Sandbox::new().allows(&tmp));
    }

    #[test]
    fn ungranted_workspace_path_is_denied() {
        let sandbox = Sandbox::new();
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(!sandbox.allows(&outside));
    }

    #[cfg(unix)]
    #[test]
    fn grant_via_symlink_allows_canonical_and_pending_dest() {
        let root = std::env::temp_dir().join(format!(
            "wzjp-scope-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let real = root.join("real");
        fs::create_dir_all(&real).unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();

        let mut sandbox = Sandbox::new();
        sandbox.grant(&alias);
        let file = real.join("x.txt");
        fs::write(&file, b"x").unwrap();
        assert!(sandbox.allows(&file));
        assert!(sandbox.allows(&alias.join("x.txt")));
        let dest = alias.join("out.zip");
        sandbox.grant(&dest);
        assert!(sandbox.allows(&dest));
        let canon_real = fs::canonicalize(&real).unwrap();
        assert!(sandbox.allows(&canon_real.join("out.zip")));
        assert!(!sandbox.allows(&root.join("outside.txt")));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_temp_dir_and_canonical_temp_are_same_grant() {
        let tmp = std::env::temp_dir();
        let canon = fs::canonicalize(&tmp).expect("temp_dir");
        let child = tmp.join(format!(
            "wzjp-mac-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&child).unwrap();
        let mut sandbox = Sandbox::new();
        sandbox.grant(&child);
        let dest = child.join("out.zip");
        sandbox.grant(&dest);
        assert!(sandbox.allows(&child));
        assert!(sandbox.allows(&dest));
        let canon_child = fs::canonicalize(&child).unwrap();
        assert!(sandbox.allows(&canon_child));
        assert!(sandbox.allows(&canon_child.join("out.zip")));
        if tmp != canon {
            assert!(sandbox.allows(&canon.join(child.file_name().unwrap())));
        }
        let _ = fs::remove_dir_all(&child);
    }

    #[test]
    fn save_dest_does_not_open_siblings() {
        let root = std::env::temp_dir().join(format!(
            "winzip-jp-save-sib-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).unwrap();
        let dest = root.join("out.zip");
        let sibling = root.join("secret.txt");
        fs::write(&sibling, b"no").unwrap();
        let mut sandbox = Sandbox::new();
        sandbox.grant_save_dest(&dest);
        assert!(sandbox.allows(&dest));
        assert!(!sandbox.allows(&sibling));
        assert!(!sandbox.allows(&root));
        let _ = fs::remove_dir_all(&root);
    }
}
