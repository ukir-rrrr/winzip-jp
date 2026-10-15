//! ZIP 一覧と解凍。読み取りは `zip` crate、名前は raw バイトを `decode_name` する。

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::encoding::{decode_name, detect_encoding, Detection, NameEncoding};
use crate::error::ZipcoreError;
use crate::write::{Progress, ProgressPhase, SecretString};

const COPY_BUF: usize = 64 * 1024;
const MAX_ENTRIES: usize = 65_535;
const MAX_TOTAL_UNCOMPRESSED: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ENTRY_UNCOMPRESSED: u64 = 4 * 1024 * 1024 * 1024;
const BOMB_MIN_UNCOMPRESSED: u64 = 64 * 1024 * 1024;
const BOMB_RATIO: u64 = 1_000;
const TMP_PREFIX: &str = ".winzip-jp-tmp-";

#[derive(Clone, Debug)]
pub struct ListRequest {
    pub archive: PathBuf,
    /// `None` なら `detect_encoding`。UI の手動切替は `Some`。
    pub encoding: Option<NameEncoding>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListedEntry {
    pub name_raw: Vec<u8>,
    pub display_name: String,
    pub is_dir: bool,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub crc: u32,
    pub gpbf: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListReport {
    pub entries: Vec<ListedEntry>,
    pub detection: Detection,
    pub encoding: NameEncoding,
}

#[derive(Clone, Debug)]
pub struct ExtractRequest {
    pub archive: PathBuf,
    pub dest: PathBuf,
    pub encoding: Option<NameEncoding>,
    pub overwrite: bool,
    pub password: Option<SecretString>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtractReport {
    pub dest: PathBuf,
    pub entries_written: u32,
    pub bytes_written: u64,
}

struct CatalogEntry {
    index: usize,
    name_raw: Vec<u8>,
    display_name: String,
    is_dir: bool,
    uncompressed_size: u64,
    compressed_size: u64,
    crc: u32,
    gpbf: u16,
}

struct Catalog {
    entries: Vec<CatalogEntry>,
    detection: Detection,
    encoding: NameEncoding,
}

pub fn list_zip(req: ListRequest) -> Result<ListReport, ZipcoreError> {
    let mut archive = open_archive(&req.archive)?;
    let catalog = catalog_archive(&mut archive, req.encoding)?;
    Ok(ListReport {
        entries: catalog
            .entries
            .into_iter()
            .map(|e| ListedEntry {
                name_raw: e.name_raw,
                display_name: e.display_name,
                is_dir: e.is_dir,
                uncompressed_size: e.uncompressed_size,
                compressed_size: e.compressed_size,
                crc: e.crc,
                gpbf: e.gpbf,
            })
            .collect(),
        detection: catalog.detection,
        encoding: catalog.encoding,
    })
}

pub fn extract_zip(
    req: ExtractRequest,
    mut progress: impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<ExtractReport, ZipcoreError> {
    progress(Progress {
        phase: ProgressPhase::Starting,
        current_entry: None,
        entries_done: 0,
        entries_total: 0,
        bytes_done: 0,
        bytes_total: 0,
    });
    check_cancel(cancel)?;
    if req.password.is_some() {
        return Err(ZipcoreError::AesNotImplemented);
    }

    let dest_canon = prepare_dest(&req.dest)?;
    let archive_canon = fs::canonicalize(&req.archive)?;
    if path_eq(&archive_canon, &dest_canon) || stays_under(&dest_canon, &archive_canon) {
        return Err(ZipcoreError::DestOverlapsInput {
            dest: req.dest.clone(),
        });
    }

    let mut archive = open_archive(&req.archive)?;
    let catalog = catalog_archive(&mut archive, req.encoding)?;
    let bytes_total: u64 = catalog
        .entries
        .iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.uncompressed_size)
        .fold(0, u64::saturating_add);
    let entries_total = catalog.entries.len() as u32;

    let planned = plan_outputs(&catalog.entries, &dest_canon, &archive_canon)?;
    if !req.overwrite {
        let existing: Vec<String> = planned
            .iter()
            .filter(|p| !p.entry.is_dir && p.dest.exists())
            .map(|p| p.entry.display_name.clone())
            .collect();
        if !existing.is_empty() {
            return Err(ZipcoreError::OverwriteRequired { paths: existing });
        }
    }

    let mut session = ExtractSession {
        dest_canon: dest_canon.clone(),
        created_files: Vec::new(),
        created_dirs: Vec::new(),
        temps: Vec::new(),
        backups: Vec::new(),
    };
    let result = extract_planned(
        &mut archive,
        &planned,
        &mut session,
        entries_total,
        bytes_total,
        &mut progress,
        cancel,
    );
    match result {
        Ok(report) => {
            session.finish();
            progress(Progress {
                phase: ProgressPhase::Done,
                current_entry: None,
                entries_done: report.entries_written,
                entries_total,
                bytes_done: report.bytes_written,
                bytes_total,
            });
            Ok(report)
        }
        Err(e) => {
            session.rollback();
            Err(e)
        }
    }
}

fn extract_planned(
    archive: &mut ZipArchive<File>,
    planned: &[PlannedOutput<'_>],
    session: &mut ExtractSession,
    entries_total: u32,
    bytes_total: u64,
    progress: &mut impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<ExtractReport, ZipcoreError> {
    let mut entries_done = 0u32;
    let mut bytes_done = 0u64;

    for item in planned {
        check_cancel(cancel)?;
        progress(Progress {
            phase: ProgressPhase::Writing,
            current_entry: Some(item.entry.display_name.clone()),
            entries_done,
            entries_total,
            bytes_done,
            bytes_total,
        });
        if item.entry.is_dir {
            session.ensure_dir(&item.dest)?;
        } else {
            if let Some(parent) = item.dest.parent() {
                session.ensure_dir(parent)?;
            }
            let written = extract_one_file(
                archive,
                item,
                session,
                progress,
                Progress {
                    phase: ProgressPhase::Writing,
                    current_entry: Some(item.entry.display_name.clone()),
                    entries_done,
                    entries_total,
                    bytes_done,
                    bytes_total,
                },
                cancel,
            )?;
            bytes_done = bytes_done.saturating_add(written);
        }
        entries_done += 1;
    }

    progress(Progress {
        phase: ProgressPhase::Finishing,
        current_entry: None,
        entries_done,
        entries_total,
        bytes_done,
        bytes_total,
    });

    Ok(ExtractReport {
        dest: session.dest_canon.clone(),
        entries_written: entries_done,
        bytes_written: bytes_done,
    })
}

fn extract_one_file(
    archive: &mut ZipArchive<File>,
    item: &PlannedOutput<'_>,
    session: &mut ExtractSession,
    progress: &mut impl FnMut(Progress),
    snap: Progress,
    cancel: &AtomicBool,
) -> Result<u64, ZipcoreError> {
    check_cancel(cancel)?;
    let existed = item.dest.exists();
    let tmp = unique_tmp(&session.dest_canon);
    session.temps.push(tmp.clone());

    let mut src = archive.by_index(item.entry.index)?;
    if src.encrypted() {
        return Err(ZipcoreError::Encrypted {
            entry: item.entry.display_name.clone(),
        });
    }
    if src.is_symlink() {
        return Err(ZipcoreError::PathTraversal {
            entry: item.entry.display_name.clone(),
        });
    }

    let mut out = File::create(&tmp)?;
    let mut hasher = crc32fast::Hasher::new();
    let mut written = 0u64;
    let mut buf = [0u8; COPY_BUF];
    loop {
        check_cancel(cancel)?;
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        written = written.saturating_add(n as u64);
        if written > item.entry.uncompressed_size || written > MAX_ENTRY_UNCOMPRESSED {
            return Err(ZipcoreError::Corrupt(format!(
                "{} の展開サイズがヘッダを超えています",
                item.entry.display_name
            )));
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n])?;
        let mut p = snap.clone();
        p.bytes_done = snap.bytes_done.saturating_add(written);
        progress(p);
    }
    drop(src);
    if written != item.entry.uncompressed_size {
        return Err(ZipcoreError::Corrupt(format!(
            "{} の展開サイズがヘッダと一致しません",
            item.entry.display_name
        )));
    }
    if hasher.finalize() != item.entry.crc {
        return Err(ZipcoreError::Corrupt(format!(
            "{} の CRC が一致しません",
            item.entry.display_name
        )));
    }
    out.sync_all()?;
    drop(out);

    let backup = place_extracted(&tmp, &item.dest)?;
    session.temps.pop();
    if let Some(backup) = backup {
        session.backups.push((item.dest.clone(), backup));
    }
    let placed = fs::canonicalize(&item.dest)?;
    if !stays_under(&placed, &session.dest_canon) {
        let _ = fs::remove_file(&item.dest);
        return Err(ZipcoreError::PathTraversal {
            entry: item.entry.display_name.clone(),
        });
    }
    if !existed {
        session.created_files.push(item.dest.clone());
    }
    Ok(written)
}

fn open_archive(path: &Path) -> Result<ZipArchive<File>, ZipcoreError> {
    if !path.exists() {
        return Err(ZipcoreError::InputMissing {
            path: path.to_path_buf(),
        });
    }
    let file = File::open(path)?;
    Ok(ZipArchive::new(file)?)
}

fn catalog_archive(
    archive: &mut ZipArchive<File>,
    encoding_override: Option<NameEncoding>,
) -> Result<Catalog, ZipcoreError> {
    let n = archive.len();
    if n > MAX_ENTRIES {
        return Err(ZipcoreError::LimitsExceeded {
            reason: format!("エントリ数が {MAX_ENTRIES} を超えています"),
        });
    }

    let mut detect_rows = Vec::with_capacity(n);
    let mut raw_entries = Vec::with_capacity(n);
    let mut total_uncompressed = 0u64;

    for i in 0..n {
        let file = archive.by_index_raw(i)?;
        let name_raw = file.name_raw().to_vec();
        let extra = file.extra_data().unwrap_or_default();
        let gpbf = file.flags().as_u16();
        let is_dir = file.is_dir() || name_raw.ends_with(b"/");
        let encrypted = file.encrypted();
        let is_symlink = file.is_symlink();
        let uncompressed_size = file.size();
        let compressed_size = file.compressed_size();
        let crc = file.crc32();
        drop(file);

        if is_symlink {
            let shown = String::from_utf8_lossy(&name_raw).into_owned();
            return Err(ZipcoreError::PathTraversal { entry: shown });
        }
        if !is_dir {
            if uncompressed_size > MAX_ENTRY_UNCOMPRESSED {
                return Err(ZipcoreError::LimitsExceeded {
                    reason: format!("1 エントリが {MAX_ENTRY_UNCOMPRESSED} バイトを超えています"),
                });
            }
            if is_zip_bomb(compressed_size, uncompressed_size) {
                return Err(ZipcoreError::LimitsExceeded {
                    reason: "圧縮率が異常です".into(),
                });
            }
            total_uncompressed = total_uncompressed.saturating_add(uncompressed_size);
            if total_uncompressed > MAX_TOTAL_UNCOMPRESSED {
                return Err(ZipcoreError::LimitsExceeded {
                    reason: format!("展開後サイズが {MAX_TOTAL_UNCOMPRESSED} バイトを超えています"),
                });
            }
        }

        detect_rows.push((gpbf, name_raw.clone(), extra));
        raw_entries.push(RawMeta {
            index: i,
            name_raw,
            is_dir,
            encrypted,
            uncompressed_size,
            compressed_size,
            crc,
            gpbf,
        });
    }

    let detection = detect_encoding(&detect_rows);
    let encoding = encoding_override.unwrap_or(detection.encoding);
    let mut entries = Vec::with_capacity(raw_entries.len());
    let mut seen_dest = std::collections::HashSet::new();
    for raw in raw_entries {
        let display_name = decode_name(&raw.name_raw, encoding);
        if let Some(reason) = unsafe_entry_reason(&raw.name_raw, &display_name) {
            return Err(ZipcoreError::PathTraversal {
                entry: format!("{display_name} ({reason})"),
            });
        }
        if raw.encrypted {
            return Err(ZipcoreError::Encrypted {
                entry: display_name,
            });
        }
        let key = display_name.replace('\\', "/");
        if !seen_dest.insert(key) {
            return Err(ZipcoreError::Corrupt(format!(
                "同じ展開先に複数のエントリがあります: {display_name}"
            )));
        }
        entries.push(CatalogEntry {
            index: raw.index,
            name_raw: raw.name_raw,
            display_name,
            is_dir: raw.is_dir,
            uncompressed_size: raw.uncompressed_size,
            compressed_size: raw.compressed_size,
            crc: raw.crc,
            gpbf: raw.gpbf,
        });
    }

    Ok(Catalog {
        entries,
        detection,
        encoding,
    })
}

struct RawMeta {
    index: usize,
    name_raw: Vec<u8>,
    is_dir: bool,
    encrypted: bool,
    uncompressed_size: u64,
    compressed_size: u64,
    crc: u32,
    gpbf: u16,
}

struct PlannedOutput<'a> {
    entry: &'a CatalogEntry,
    dest: PathBuf,
}

fn plan_outputs<'a>(
    entries: &'a [CatalogEntry],
    dest_canon: &Path,
    archive_canon: &Path,
) -> Result<Vec<PlannedOutput<'a>>, ZipcoreError> {
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let dest = join_under_dest(dest_canon, &entry.display_name, entry.is_dir)?;
        if path_eq(&dest, archive_canon) || stays_under(archive_canon, &dest) {
            return Err(ZipcoreError::DestOverlapsInput { dest: dest.clone() });
        }
        out.push(PlannedOutput { entry, dest });
    }
    Ok(out)
}

fn join_under_dest(
    dest_canon: &Path,
    display_name: &str,
    is_dir: bool,
) -> Result<PathBuf, ZipcoreError> {
    let unified = display_name.replace('\\', "/");
    let trimmed = if is_dir {
        unified.trim_end_matches('/')
    } else {
        unified.as_str()
    };
    let mut current = dest_canon.to_path_buf();
    for component in trimmed.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if unsafe_component(component) {
            return Err(ZipcoreError::PathTraversal {
                entry: display_name.to_string(),
            });
        }
        current.push(component);
        if !lexical_stays_under(&current, dest_canon) {
            return Err(ZipcoreError::PathTraversal {
                entry: display_name.to_string(),
            });
        }
        if current.exists() {
            let canon = fs::canonicalize(&current)?;
            if !stays_under(&canon, dest_canon) {
                return Err(ZipcoreError::PathTraversal {
                    entry: display_name.to_string(),
                });
            }
            if fs::symlink_metadata(&current)?.file_type().is_symlink() {
                return Err(ZipcoreError::PathTraversal {
                    entry: display_name.to_string(),
                });
            }
        }
    }
    if !lexical_stays_under(&current, dest_canon) {
        return Err(ZipcoreError::PathTraversal {
            entry: display_name.to_string(),
        });
    }
    Ok(current)
}

/// raw と表示名の両方を見る。区切りは ZIP 上では ASCII の `/` と `\`。
pub(crate) fn unsafe_entry_reason(raw: &[u8], display: &str) -> Option<&'static str> {
    if raw.contains(&0) || display.contains('\0') {
        return Some("NUL");
    }
    if raw_escapes(raw) {
        return Some("raw");
    }
    if display_escapes(display) {
        return Some("name");
    }
    None
}

fn raw_escapes(raw: &[u8]) -> bool {
    if raw.is_empty() {
        return false;
    }
    if raw[0] == b'/' || raw[0] == b'\\' {
        return true;
    }
    if raw.len() >= 2 && raw[1] == b':' && raw[0].is_ascii_alphabetic() {
        return true;
    }
    for part in raw.split(|b| *b == b'/' || *b == b'\\') {
        if part == b".." || part.contains(&0) {
            return true;
        }
        if is_reserved_dos_bytes(part) {
            return true;
        }
    }
    false
}

fn display_escapes(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let unified = name.replace('\\', "/");
    if unified.starts_with('/') {
        return true;
    }
    if looks_like_drive(&unified) {
        return true;
    }
    for component in Path::new(&unified).components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return true,
            Component::Normal(s) => {
                let t = s.to_string_lossy();
                if unsafe_component(&t) {
                    return true;
                }
            }
            Component::CurDir => {}
        }
    }
    unified
        .split('/')
        .filter(|c| !c.is_empty() && *c != ".")
        .any(unsafe_component)
}

fn unsafe_component(component: &str) -> bool {
    if component == "." {
        return false;
    }
    if component.is_empty() || component == ".." {
        return true;
    }
    if component.contains('\0') || component.contains('\\') || component.contains('/') {
        return true;
    }
    if looks_like_drive(component) {
        return true;
    }
    if is_reserved_dos_name(component) {
        return true;
    }
    let trimmed = component.trim_end_matches([' ', '.']);
    trimmed.is_empty() || trimmed != component
}

fn looks_like_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()
}

fn is_reserved_dos_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn is_reserved_dos_bytes(part: &[u8]) -> bool {
    let stem = part.split(|b| *b == b'.').next().unwrap_or(part);
    is_reserved_dos_name(&String::from_utf8_lossy(stem))
}

fn is_zip_bomb(compressed: u64, uncompressed: u64) -> bool {
    if uncompressed <= BOMB_MIN_UNCOMPRESSED {
        return false;
    }
    if compressed == 0 {
        return true;
    }
    uncompressed / compressed > BOMB_RATIO
}

fn prepare_dest(dest: &Path) -> Result<PathBuf, ZipcoreError> {
    let abs = absolute(dest);
    if !abs.exists() {
        fs::create_dir_all(&abs)?;
    }
    let canon = fs::canonicalize(&abs).map_err(|_| ZipcoreError::NotADirectory { path: abs })?;
    if !canon.is_dir() {
        return Err(ZipcoreError::NotADirectory { path: canon });
    }
    Ok(canon)
}

struct ExtractSession {
    dest_canon: PathBuf,
    created_files: Vec<PathBuf>,
    created_dirs: Vec<PathBuf>,
    temps: Vec<PathBuf>,
    /// 上書き前の退避。成功時に消し、失敗時は戻す。
    backups: Vec<(PathBuf, PathBuf)>,
}

impl ExtractSession {
    fn ensure_dir(&mut self, dir: &Path) -> Result<(), ZipcoreError> {
        if !lexical_stays_under(dir, &self.dest_canon) {
            return Err(ZipcoreError::PathTraversal {
                entry: dir.display().to_string(),
            });
        }
        let mut missing = Vec::new();
        let mut cur = dir.to_path_buf();
        loop {
            if cur.exists() {
                break;
            }
            missing.push(cur.clone());
            if cur == self.dest_canon {
                break;
            }
            match cur.parent() {
                Some(parent) => cur = parent.to_path_buf(),
                None => break,
            }
        }
        if cur.exists() {
            let canon = fs::canonicalize(&cur)?;
            if !stays_under(&canon, &self.dest_canon) {
                return Err(ZipcoreError::PathTraversal {
                    entry: dir.display().to_string(),
                });
            }
            if fs::symlink_metadata(&cur)?.file_type().is_symlink() {
                return Err(ZipcoreError::PathTraversal {
                    entry: dir.display().to_string(),
                });
            }
        }
        for d in missing.iter().rev() {
            fs::create_dir(d)?;
            self.created_dirs.push(d.clone());
            let canon = fs::canonicalize(d)?;
            if !stays_under(&canon, &self.dest_canon) {
                return Err(ZipcoreError::PathTraversal {
                    entry: d.display().to_string(),
                });
            }
        }
        Ok(())
    }

    fn finish(self) {
        for (_, backup) in self.backups {
            let _ = fs::remove_file(backup);
        }
    }

    fn rollback(&self) {
        for tmp in self.temps.iter().rev() {
            let _ = fs::remove_file(tmp);
        }
        for (dest, backup) in self.backups.iter().rev() {
            let _ = fs::rename(backup, dest);
        }
        for f in self.created_files.iter().rev() {
            let _ = fs::remove_file(f);
        }
        for d in self.created_dirs.iter().rev() {
            let _ = fs::remove_dir(d);
        }
    }
}

fn unique_tmp(dest: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dest.join(format!(
        "{TMP_PREFIX}{}-{}-{}",
        std::process::id(),
        n,
        nanos
    ))
}

fn place_extracted(from: &Path, to: &Path) -> io::Result<Option<PathBuf>> {
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

fn unique_sidecar(path: &Path, kind: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{kind}.{}.{}", std::process::id(), n));
    PathBuf::from(name)
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), ZipcoreError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ZipcoreError::Cancelled)
    } else {
        Ok(())
    }
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn strip_verbatim(path: &Path) -> PathBuf {
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

fn normalize_lexically(path: &Path) -> PathBuf {
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

fn path_eq(a: &Path, b: &Path) -> bool {
    let a = strip_verbatim(&normalize_lexically(a));
    let b = strip_verbatim(&normalize_lexically(b));
    a == b || a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

fn lexical_stays_under(child: &Path, parent: &Path) -> bool {
    stays_under(&normalize_lexically(child), &normalize_lexically(parent))
}

fn stays_under(child: &Path, parent: &Path) -> bool {
    let child = strip_verbatim(child);
    let parent = strip_verbatim(parent);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_dotdot_and_slash_are_unsafe() {
        assert!(unsafe_entry_reason(b"../evil", "../evil").is_some());
        assert!(unsafe_entry_reason(b"/tmp/evil", "/tmp/evil").is_some());
        assert!(unsafe_entry_reason(br"C:\evil", r"C:\evil").is_some());
        assert!(unsafe_entry_reason(br"foo\..\evil", r"foo\..\evil").is_some());
        assert!(unsafe_entry_reason(b"foo/../bar", "foo/../bar").is_some());
        assert!(unsafe_entry_reason(b"ok/file.txt", "ok/file.txt").is_none());
        assert!(unsafe_entry_reason(b"dir/", "dir/").is_none());
        assert!(unsafe_entry_reason("が.txt".as_bytes(), "が.txt").is_none());
    }

    #[test]
    fn nul_and_reserved_are_unsafe() {
        assert!(unsafe_entry_reason(b"a\0b", "a\0b").is_some());
        assert!(unsafe_entry_reason(b"NUL", "NUL").is_some());
        assert!(unsafe_entry_reason(b"CON.txt", "CON.txt").is_some());
        assert!(unsafe_entry_reason(b"hello.txt", "hello.txt").is_none());
    }
}
