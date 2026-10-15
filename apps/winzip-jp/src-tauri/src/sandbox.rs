//! セッション内のユーザー選択パスと、コンテナ一時ファイルの公開。
//!
//! アクセスが開く経路はダイアログとドロップだけ。永続 grant は持たない。

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use zipcore::{absolute, normalize_lexically, path_eq, resolve_path, stays_under};

const TMP_PREFIX: &str = "winzip-jp-";
pub(crate) const RESTORE_BACKUP_HINT: &str = "元の保存先の退避: ";

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
}

impl Sandbox {
    pub fn new() -> Self {
        Self::default()
    }

    /// ダイアログ / ドロップで選んだ入力。ディレクトリなら配下も許可。
    /// シンボリックリンク自体は拒否し、リンク先ツリーを開かない。
    pub fn grant(&mut self, path: &Path) -> bool {
        let abs = normalize_lexically(&absolute(path));
        if is_symlink(&abs) {
            return false;
        }
        let stored = resolve_path(&abs);
        let recursive = stored.is_dir() || abs.is_dir();
        self.insert(abs, stored, recursive);
        true
    }

    pub fn grant_many<P: AsRef<Path>>(&mut self, paths: impl IntoIterator<Item = P>) {
        for path in paths {
            let _ = self.grant(path.as_ref());
        }
    }

    /// 保存先 ZIP。ファイルだけ許可する。親フォルダはセッションにもカーネルにも開かない。
    pub fn grant_save_dest(&mut self, path: &Path) -> bool {
        let abs = normalize_lexically(&absolute(path));
        if is_symlink(&abs) {
            return false;
        }
        let stored = resolve_path(&abs);
        self.insert(abs, stored, false);
        true
    }

    fn insert(&mut self, original: PathBuf, stored: PathBuf, recursive: bool) {
        if let Some(existing) = self.grants.iter_mut().find(|g| path_eq(&g.path, &stored)) {
            existing.recursive |= recursive;
            return;
        }
        // startAccessing はダイアログ/ドロップの元パス。比較キーは canonicalize 後。
        #[cfg(target_os = "macos")]
        let access = scoped::ScopedUrl::start(&original, stored.is_dir() || original.is_dir());
        #[cfg(not(target_os = "macos"))]
        let _ = original;
        self.grants.push(Grant {
            path: stored,
            recursive,
            #[cfg(target_os = "macos")]
            _access: access,
        });
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

    /// 保存先 ZIP は完全一致の grant だけ。入力フォルダ配下だからといって上書きしない。
    pub fn resolve_save_file(&self, path: &Path) -> Result<PathBuf, String> {
        let raw = path.to_string_lossy();
        if raw.is_empty() || raw.contains('\0') {
            return Err("パスが不正です".into());
        }
        let resolved = resolve_path(path);
        if resolved.is_dir() {
            return Err("保存先がフォルダです".into());
        }
        if self.grants.iter().any(|g| path_eq(&resolved, &g.path)) {
            return Ok(resolved);
        }
        Err(format!(
            "この場所にはアクセスできません: {}",
            resolved.display()
        ))
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
        }
    }
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
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
    let Some(parent) = abs.parent() else {
        return false;
    };
    if !path_eq(parent, &tmp) {
        return false;
    }
    let name = abs.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    name.starts_with(TMP_PREFIX)
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
    match fs::rename(tmp, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => publish_across_volumes(tmp, dest),
        Err(e) => {
            discard_tmp(tmp);
            Err(e)
        }
    }
}

/// 別ボリュームでは dest を直接 copy する。失敗時は既存 dest をコンテナ退避から戻す。
/// 戻しに失敗したら退避は消さない。
fn publish_across_volumes(tmp: &Path, dest: &Path) -> io::Result<()> {
    let backup = dest.exists().then(container_tmp_zip);
    publish_across_volumes_inner(tmp, dest, backup)
}

fn publish_across_volumes_inner(
    tmp: &Path,
    dest: &Path,
    backup: Option<PathBuf>,
) -> io::Result<()> {
    if let Some(bak) = &backup {
        if let Err(e) = fs::copy(dest, bak) {
            discard_tmp(tmp);
            return Err(e);
        }
    }
    match copy_tmp_to_dest(tmp, dest) {
        Ok(_) => {
            discard_tmp(tmp);
            if let Some(bak) = backup {
                let _ = fs::remove_file(bak);
            }
            Ok(())
        }
        Err(e) => {
            if let Some(bak) = backup {
                if !restore_or_keep_backup(&bak, dest) {
                    discard_tmp(tmp);
                    return Err(io::Error::new(
                        e.kind(),
                        format!("{RESTORE_BACKUP_HINT}{}", bak.display()),
                    ));
                }
            } else {
                let _ = fs::remove_file(dest);
            }
            discard_tmp(tmp);
            Err(e)
        }
    }
}

fn copy_tmp_to_dest(tmp: &Path, dest: &Path) -> io::Result<u64> {
    #[cfg(test)]
    if FAIL_AFTER_TRUNCATE.with(|c| c.replace(false)) {
        let _ = File::create(dest);
        let _ = fs::remove_file(dest);
        let _ = fs::create_dir(dest);
        return Err(io::Error::new(
            io::ErrorKind::StorageFull,
            "copy failed after truncate",
        ));
    }
    fs::copy(tmp, dest)
}

fn restore_or_keep_backup(bak: &Path, dest: &Path) -> bool {
    if fs::copy(bak, dest).is_ok() {
        let _ = fs::remove_file(bak);
        true
    } else {
        false
    }
}

#[cfg(test)]
thread_local! {
    static FAIL_AFTER_TRUNCATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
        let nested = std::env::temp_dir()
            .join("other")
            .join(format!("{TMP_PREFIX}nested.zip"));
        assert!(!is_container_tmp(&nested));
    }

    #[test]
    fn ungranted_workspace_path_is_denied() {
        let sandbox = Sandbox::new();
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(!sandbox.allows(&outside));
    }

    #[cfg(unix)]
    #[test]
    fn dropped_symlink_does_not_grant_target_tree() {
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
        let file = real.join("x.txt");
        fs::write(&file, b"x").unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();

        let mut sandbox = Sandbox::new();
        assert!(!sandbox.grant(&alias));
        let alias_slash = PathBuf::from(format!("{}/", alias.display()));
        assert!(!sandbox.grant(&alias_slash));
        assert!(!sandbox.allows(&file));
        assert!(!sandbox.allows(&alias.join("x.txt")));
        assert!(sandbox.grant(&real));
        assert!(sandbox.allows(&file));
        let dest = real.join("out.zip");
        assert!(sandbox.grant_save_dest(&dest));
        assert!(sandbox.allows(&dest));
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
    fn publish_tmp_overwrites_dest_without_sidecar() {
        let root = std::env::temp_dir().join(format!(
            "winzip-jp-pub-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).unwrap();
        let dest = root.join("out.zip");
        fs::write(&dest, b"old").unwrap();
        let tmp = container_tmp_zip();
        fs::write(&tmp, b"new").unwrap();
        publish_tmp(&tmp, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        assert!(!tmp.exists());
        let leftover: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftover, vec!["out.zip".to_string()]);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn publish_across_volumes_keeps_old_dest_when_copy_cannot_write() {
        use std::os::unix::fs::PermissionsExt;
        let dest_root = std::env::temp_dir().join(format!(
            "wzjp-xdev-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dest_root).unwrap();
        let dest = dest_root.join("out.zip");
        fs::write(&dest, b"old").unwrap();
        let tmp = container_tmp_zip();
        fs::write(&tmp, b"new").unwrap();
        let mut perm = fs::metadata(&dest).unwrap().permissions();
        perm.set_mode(0o444);
        fs::set_permissions(&dest, perm).unwrap();
        assert!(publish_across_volumes(&tmp, &dest).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"old");
        assert!(!tmp.exists());
        let mut perm = fs::metadata(&dest).unwrap().permissions();
        perm.set_mode(0o644);
        fs::set_permissions(&dest, perm).unwrap();
        let _ = fs::remove_dir_all(&dest_root);
    }

    #[test]
    fn publish_across_volumes_overwrites_without_parent_sidecar() {
        let dest_root = std::env::temp_dir().join(format!(
            "wzjp-xdev-ok-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dest_root).unwrap();
        let dest = dest_root.join("out.zip");
        fs::write(&dest, b"old").unwrap();
        let tmp = container_tmp_zip();
        fs::write(&tmp, b"new").unwrap();
        publish_across_volumes(&tmp, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        assert!(!tmp.exists());
        let leftover: Vec<_> = fs::read_dir(&dest_root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftover, vec!["out.zip".to_string()]);
        let _ = fs::remove_dir_all(&dest_root);
    }

    #[test]
    fn restore_keeps_backup_when_dest_cannot_be_written() {
        let dest_root = std::env::temp_dir().join(format!(
            "wzjp-restore-keep-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dest_root).unwrap();
        let dest = dest_root.join("out.zip");
        fs::create_dir_all(&dest).unwrap();
        let bak = container_tmp_zip();
        fs::write(&bak, b"old").unwrap();
        restore_or_keep_backup(&bak, &dest);
        assert!(bak.exists());
        assert_eq!(fs::read(&bak).unwrap(), b"old");
        let _ = fs::remove_file(&bak);
        let _ = fs::remove_dir_all(&dest_root);
    }

    #[test]
    fn publish_across_volumes_keeps_backup_when_restore_fails() {
        let dest_root = std::env::temp_dir().join(format!(
            "wzjp-xdev-restore-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dest_root).unwrap();
        let dest = dest_root.join("out.zip");
        fs::write(&dest, b"old").unwrap();
        let tmp = container_tmp_zip();
        fs::write(&tmp, b"new").unwrap();
        let bak = container_tmp_zip();
        FAIL_AFTER_TRUNCATE.with(|c| c.set(true));
        let err = publish_across_volumes_inner(&tmp, &dest, Some(bak.clone()));
        FAIL_AFTER_TRUNCATE.with(|c| c.set(false));
        let err = err.unwrap_err();
        assert!(err.to_string().contains(&bak.display().to_string()));
        assert!(!tmp.exists());
        assert!(bak.exists());
        assert_eq!(fs::read(&bak).unwrap(), b"old");
        let _ = fs::remove_file(&bak);
        let _ = fs::remove_dir_all(&dest_root);
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
        assert!(sandbox.resolve_save_file(&dest).is_ok());
        assert!(!sandbox.allows(&sibling));
        assert!(!sandbox.allows(&root));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn folder_grant_does_not_imply_save_file() {
        let root = std::env::temp_dir().join(format!(
            "wzjp-save-exact-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).unwrap();
        let dest = root.join("out.zip");
        let mut sandbox = Sandbox::new();
        assert!(sandbox.grant(&root));
        assert!(sandbox.allows(&dest));
        assert!(sandbox.resolve_save_file(&dest).is_err());
        assert!(sandbox.grant_save_dest(&dest));
        assert!(sandbox.resolve_save_file(&dest).is_ok());
        let _ = fs::remove_dir_all(&root);
    }
}
