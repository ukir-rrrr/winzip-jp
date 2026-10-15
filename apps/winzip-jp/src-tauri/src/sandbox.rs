//! セッション内のユーザー選択パスと、コンテナ一時ファイルの公開。
//!
//! アクセスが開く経路はダイアログとドロップだけ。永続 grant は持たない。

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use zipcore::split::{self, is_split_sibling};
use zipcore::{absolute, normalize_lexically, path_eq, resolve_path, stays_under};

const TMP_PREFIX: &str = "winzip-jp-";

#[derive(Debug)]
pub enum PublishError {
    Io(io::Error),
    BackupKept { source: io::Error, backup: PathBuf },
}

impl From<io::Error> for PublishError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

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
        if let Some(existing) = self.grants.iter_mut().find(|g| g.path == stored) {
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
            if path == g.path {
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
        if self.grants.iter().any(|g| g.path == resolved) {
            return Ok(resolved);
        }
        if self
            .grants
            .iter()
            .any(|g| !g.recursive && is_split_sibling(&g.path, &resolved))
        {
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

/// コンテナの分割ピースを、同じ stem の最終パスへ順に公開する。
/// 失敗時は今回作った dest だけ消し、もともとあった dest は退避から戻す。
pub fn publish_tmp_set(tmp_vols: &[PathBuf], dest_vols: &[PathBuf]) -> Result<(), PublishError> {
    if tmp_vols.len() != dest_vols.len() || tmp_vols.is_empty() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "分割ピースの数が合いません").into(),
        );
    }
    let mut created: Vec<PathBuf> = Vec::new();
    let mut backups: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (tmp, dest) in tmp_vols.iter().zip(dest_vols.iter()) {
        let backup = if dest.exists() {
            let bak = container_tmp_zip();
            if let Err(e) = fs::copy(dest, &bak) {
                let _ = fs::remove_file(&bak);
                rollback_publish_set(&created, &backups, tmp_vols);
                return Err(e.into());
            }
            Some(bak)
        } else {
            None
        };
        match publish_tmp(tmp, dest) {
            Ok(()) => {
                if let Some(bak) = backup {
                    backups.push((dest.clone(), bak));
                } else {
                    created.push(dest.clone());
                }
            }
            Err(e) => {
                if let Some(bak) = backup {
                    let _ = fs::remove_file(&bak);
                }
                rollback_publish_set(&created, &backups, tmp_vols);
                return Err(e);
            }
        }
    }
    for (_, bak) in backups {
        let _ = fs::remove_file(bak);
    }
    if let Some(dest) = dest_vols.last() {
        split::discard_stale_volumes(dest, dest_vols.len() as u32);
    }
    Ok(())
}

fn rollback_publish_set(created: &[PathBuf], backups: &[(PathBuf, PathBuf)], tmp_vols: &[PathBuf]) {
    for p in created {
        let _ = fs::remove_file(p);
    }
    for (dest, bak) in backups {
        let _ = restore_or_keep_backup(bak, dest);
    }
    for t in tmp_vols {
        remove_tmp_file(t);
    }
}

/// コンテナ tmp を fsync してからユーザー先へ rename（別ボリュームは copy+remove）。
pub fn publish_tmp(tmp: &Path, dest: &Path) -> Result<(), PublishError> {
    if !is_container_tmp(tmp) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "コンテナ一時ファイルではありません",
        )
        .into());
    }
    if tmp.exists() {
        File::open(tmp)?.sync_all()?;
    }
    match fs::rename(tmp, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => publish_across_volumes(tmp, dest),
        Err(e) => {
            remove_tmp_file(tmp);
            Err(e.into())
        }
    }
}

/// 別ボリュームでは dest を直接 copy する。失敗時は既存 dest をコンテナ退避から戻す。
/// 戻しに失敗したら退避は消さない。
fn publish_across_volumes(tmp: &Path, dest: &Path) -> Result<(), PublishError> {
    publish_across_volumes_with(
        tmp,
        dest,
        |src, dst| fs::copy(src, dst).map(|_| ()),
        sync_file,
    )
}

fn publish_across_volumes_with(
    tmp: &Path,
    dest: &Path,
    place: impl FnOnce(&Path, &Path) -> io::Result<()>,
    persist: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<(), PublishError> {
    let backup = if dest.exists() {
        let bak = container_tmp_zip();
        if let Err(e) = fs::copy(dest, &bak) {
            let _ = fs::remove_file(&bak);
            remove_tmp_file(tmp);
            return Err(e.into());
        }
        Some(bak)
    } else {
        None
    };
    match place(tmp, dest) {
        Ok(()) => {
            if let Err(e) = persist(dest) {
                remove_tmp_file(tmp);
                return match backup {
                    Some(bak) => Err(PublishError::BackupKept {
                        source: e,
                        backup: bak,
                    }),
                    None => {
                        let _ = fs::remove_file(dest);
                        Err(e.into())
                    }
                };
            }
            remove_tmp_file(tmp);
            if let Some(bak) = backup {
                let _ = fs::remove_file(bak);
            }
            Ok(())
        }
        Err(e) => {
            if let Some(bak) = backup {
                if restore_or_keep_backup(&bak, dest) {
                    remove_tmp_file(tmp);
                    Err(PublishError::Io(e))
                } else {
                    remove_tmp_file(tmp);
                    Err(PublishError::BackupKept {
                        source: e,
                        backup: bak,
                    })
                }
            } else {
                let _ = fs::remove_file(dest);
                remove_tmp_file(tmp);
                Err(e.into())
            }
        }
    }
}

fn sync_file(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn restore_or_keep_backup(bak: &Path, dest: &Path) -> bool {
    if fs::copy(bak, dest).is_err() {
        return false;
    }
    if sync_file(dest).is_err() {
        return false;
    }
    let _ = fs::remove_file(bak);
    true
}

/// 1 本のコンテナ一時ファイルだけ消す。`.zNN` に対して兄弟ピースは見ない。
fn remove_tmp_file(tmp: &Path) {
    if !is_container_tmp(tmp) {
        return;
    }
    let _ = fs::remove_file(tmp);
    let mut partial = tmp.as_os_str().to_os_string();
    partial.push(".partial");
    let _ = fs::remove_file(partial);
}

fn is_zip_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// 最後の `.zip` を渡す。同じ stem の分割ピースと `.partial` をまとめて消す。
/// `.zNN` を渡しても兄弟は消さない（公開中の 1 ピース失敗でセットを壊さない）。
/// `.z01` が先に公開済みでも、残りの `.zNN` は stem から列挙して消す。
pub fn discard_tmp(tmp: &Path) {
    if !is_container_tmp(tmp) {
        return;
    }
    if is_zip_ext(tmp) {
        for i in 1..=split::MAX_PIECES {
            remove_tmp_file(&split::volume_path(tmp, i, false));
        }
        split::discard_partials_for(tmp);
    }
    remove_tmp_file(tmp);
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
        let err = publish_across_volumes_with(
            &tmp,
            &dest,
            |_, dest| {
                let _ = File::create(dest);
                let _ = fs::remove_file(dest);
                let _ = fs::create_dir(dest);
                Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "copy failed after truncate",
                ))
            },
            sync_file,
        );
        let PublishError::BackupKept { backup, .. } = err.unwrap_err() else {
            panic!("expected BackupKept");
        };
        assert!(!tmp.exists());
        assert!(backup.exists());
        assert_eq!(fs::read(&backup).unwrap(), b"old");
        let _ = fs::remove_file(&backup);
        let _ = fs::remove_dir_all(&dest_root);
    }

    #[test]
    fn new_dest_is_removed_when_persist_fails() {
        let dest_root = std::env::temp_dir().join(format!(
            "wzjp-xdev-new-sync-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dest_root).unwrap();
        let dest = dest_root.join("out.zip");
        let tmp = container_tmp_zip();
        fs::write(&tmp, b"new").unwrap();
        let err = publish_across_volumes_with(
            &tmp,
            &dest,
            |src, dst| fs::copy(src, dst).map(|_| ()),
            |_| Err(io::Error::new(io::ErrorKind::StorageFull, "persist failed")),
        );
        assert!(err.is_err());
        assert!(!dest.exists());
        assert!(!tmp.exists());
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
        let other_case = root.join("OUT.zip");
        assert!(sandbox.resolve_save_file(&other_case).is_err());
        assert!(!sandbox.allows(&sibling));
        assert!(!sandbox.allows(&root));
        let z01 = root.join("out.z01");
        assert!(sandbox.resolve_save_file(&z01).is_ok());
        assert!(!sandbox.allows(&z01));
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

    #[test]
    fn publish_across_volumes_does_not_delete_other_tmp_volumes() {
        let dest_root = std::env::temp_dir().join(format!(
            "wzjp-xdev-split-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dest_root).unwrap();
        let dest = dest_root.join("out.z01");
        let tmp_zip = container_tmp_zip();
        let tmp_z01 = split::volume_path(&tmp_zip, 1, false);
        let tmp_z02 = split::volume_path(&tmp_zip, 2, false);
        fs::write(&tmp_z01, b"1").unwrap();
        fs::write(&tmp_z02, b"2").unwrap();
        fs::write(&tmp_zip, b"3").unwrap();
        publish_across_volumes(&tmp_z01, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"1");
        assert!(!tmp_z01.exists());
        assert!(tmp_z02.exists(), "z02 must survive discarding z01");
        assert!(tmp_zip.exists(), "last zip tmp must survive discarding z01");
        discard_tmp(&tmp_zip);
        assert!(!tmp_z02.exists());
        assert!(!tmp_zip.exists());
        let _ = fs::remove_dir_all(&dest_root);
    }

    #[test]
    fn discard_tmp_on_z01_does_not_delete_sibling_volumes() {
        let tmp_zip = container_tmp_zip();
        let tmp_z01 = split::volume_path(&tmp_zip, 1, false);
        let tmp_z02 = split::volume_path(&tmp_zip, 2, false);
        fs::write(&tmp_z01, b"1").unwrap();
        fs::write(&tmp_z02, b"2").unwrap();
        fs::write(&tmp_zip, b"3").unwrap();
        discard_tmp(&tmp_z01);
        assert!(!tmp_z01.exists());
        assert!(
            tmp_z02.exists(),
            "discard_tmp(.z01) must not expand siblings"
        );
        assert!(
            tmp_zip.exists(),
            "discard_tmp(.z01) must not delete last zip"
        );
        discard_tmp(&tmp_zip);
        assert!(!tmp_z02.exists());
        assert!(!tmp_zip.exists());
    }

    #[test]
    fn publish_tmp_set_failure_restores_existing_z01() {
        let dest_root = std::env::temp_dir().join(format!(
            "wzjp-pubset-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dest_root).unwrap();
        let dest_zip = dest_root.join("out.zip");
        let dest_z01 = dest_root.join("out.z01");
        let dest_z02 = dest_root.join("out.z02");
        fs::write(&dest_z01, b"OLDZ01").unwrap();
        fs::write(&dest_zip, b"OLDZIP").unwrap();
        fs::create_dir(&dest_z02).unwrap();
        let tmp_zip = container_tmp_zip();
        let tmp_z01 = split::volume_path(&tmp_zip, 1, false);
        let tmp_z02 = split::volume_path(&tmp_zip, 2, false);
        fs::write(&tmp_z01, b"new1").unwrap();
        fs::write(&tmp_z02, b"new2").unwrap();
        fs::write(&tmp_zip, b"new3").unwrap();
        assert!(publish_tmp_set(
            &[tmp_z01, tmp_z02, tmp_zip],
            &[dest_z01.clone(), dest_z02.clone(), dest_zip.clone()],
        )
        .is_err());
        assert_eq!(fs::read(&dest_z01).unwrap(), b"OLDZ01");
        assert_eq!(fs::read(&dest_zip).unwrap(), b"OLDZIP");
        assert!(dest_z02.is_dir());
        let _ = fs::remove_dir_all(&dest_root);
    }

    #[test]
    fn publish_tmp_set_drops_stale_z_pieces() {
        let dest_root = std::env::temp_dir().join(format!(
            "wzjp-pubstale-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dest_root).unwrap();
        let dest_zip = dest_root.join("out.zip");
        let dest_z01 = dest_root.join("out.z01");
        let dest_z02 = dest_root.join("out.z02");
        fs::write(&dest_z01, b"OLD1").unwrap();
        fs::write(&dest_z02, b"OLD2").unwrap();
        fs::write(&dest_zip, b"OLDZIP").unwrap();
        let tmp_zip = container_tmp_zip();
        let tmp_z01 = split::volume_path(&tmp_zip, 1, false);
        fs::write(&tmp_z01, b"new1").unwrap();
        fs::write(&tmp_zip, b"newzip").unwrap();
        publish_tmp_set(&[tmp_z01, tmp_zip], &[dest_z01.clone(), dest_zip.clone()]).unwrap();
        assert_eq!(fs::read(&dest_z01).unwrap(), b"new1");
        assert_eq!(fs::read(&dest_zip).unwrap(), b"newzip");
        assert!(!dest_z02.exists());
        let _ = fs::remove_dir_all(&dest_root);
    }
}
