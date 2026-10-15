//! ZIP 作成（非暗号・非分割）。ファイル名は `encode_entry_name` の CP932 バイトのみ。

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::encoding::{encode_entry_name, is_junk, Unmappable};
use crate::error::ZipcoreError;
use crate::pkzip::{
    inspect_zip, patch_local_sizes, write_central_header, write_eocd, write_local_header,
    CentralRecord, GPBF_PLAIN, METHOD_DEFLATE, METHOD_STORED,
};

pub use crate::pkzip::InspectedEntry;

const COPY_BUF: usize = 64 * 1024;
const MAX_COPY_SUFFIX: u32 = 10_000;
const DEFLATE_LEVEL: u32 = 6;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AesMode {
    #[default]
    Aes256,
}

/// パスワード。Debug に中身を出さない。AES 書き込みは PR 6。
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(***)")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputItem {
    pub abs_path: PathBuf,
    pub as_dir: bool,
}

#[derive(Clone, Debug)]
pub struct WriteRequest {
    pub inputs: Vec<InputItem>,
    pub dest: PathBuf,
    pub password: Option<SecretString>,
    pub aes: AesMode,
    pub split_bytes: Option<u64>,
    pub replacements: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedEntry {
    pub source: PathBuf,
    /// 衝突連番込みの NFC 表示名。ディレクトリは末尾 `/`。
    pub relative_path: String,
    /// `encode_entry_name` が出した CP932 バイト。
    pub name_raw: Vec<u8>,
    pub is_dir: bool,
    pub uncompressed_size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub entries: Vec<PlannedEntry>,
    pub issues: Vec<Unmappable>,
    pub excluded: Vec<String>,
    pub total_uncompressed: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgressPhase {
    Starting,
    Writing,
    Finishing,
    Done,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Progress {
    pub phase: ProgressPhase,
    pub current_entry: Option<String>,
    pub entries_done: u32,
    pub entries_total: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WriteReport {
    pub dest: PathBuf,
    pub entries_written: u32,
    pub bytes_written: u64,
    pub excluded: Vec<String>,
}

pub fn inspect_zip_headers(path: &Path) -> Result<Vec<InspectedEntry>, ZipcoreError> {
    inspect_zip(path)
}

pub fn plan_write(req: &WriteRequest) -> Result<Plan, ZipcoreError> {
    let mut collected = Vec::new();
    let mut excluded = Vec::new();
    for item in &req.inputs {
        collect_item(item, &mut collected, &mut excluded)?;
    }
    if collected.is_empty() {
        return Err(ZipcoreError::EmptyInputs);
    }
    collected.sort_by(|a, b| a.nfc_original.cmp(&b.nfc_original));
    assign_names(collected, &req.replacements, excluded)
}

pub fn write_zip(
    req: WriteRequest,
    mut progress: impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<WriteReport, ZipcoreError> {
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
    let _ = req.aes;
    if req.split_bytes.is_some() {
        return Err(ZipcoreError::SplitNotImplemented);
    }

    let plan = plan_write(&req)?;
    if !plan.issues.is_empty() {
        return Err(ZipcoreError::UnconfirmedReplacements {
            issues: plan.issues,
        });
    }
    if plan.entries.len() > u16::MAX as usize {
        return Err(ZipcoreError::Zip64Required {
            reason: "エントリ数が 65535 を超えています".into(),
        });
    }
    if dest_overlaps_input(&req.dest, &req.inputs) {
        return Err(ZipcoreError::DestOverlapsInput {
            dest: req.dest.clone(),
        });
    }

    let dest = req.dest.clone();
    let result = write_plan_to_path(&plan, &dest, &mut progress, cancel);
    match result {
        Ok(report) => {
            progress(Progress {
                phase: ProgressPhase::Done,
                current_entry: None,
                entries_done: report.entries_written,
                entries_total: report.entries_written,
                bytes_done: report.bytes_written,
                bytes_total: report.bytes_written,
            });
            Ok(report)
        }
        Err(e) => {
            let _ = fs::remove_file(&dest);
            Err(e)
        }
    }
}

fn write_plan_to_path(
    plan: &Plan,
    dest: &Path,
    progress: &mut impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<WriteReport, ZipcoreError> {
    let mut out = File::create(dest)?;
    let mut centrals = Vec::with_capacity(plan.entries.len());
    let mut entries_done = 0u32;
    let mut bytes_done = 0u64;
    let entries_total = plan.entries.len() as u32;
    let bytes_total = plan.total_uncompressed;

    for entry in &plan.entries {
        check_cancel(cancel)?;
        progress(Progress {
            phase: ProgressPhase::Writing,
            current_entry: Some(entry.relative_path.clone()),
            entries_done,
            entries_total,
            bytes_done,
            bytes_total,
        });
        let rec = write_one_entry(
            &mut out,
            entry,
            cancel,
            progress,
            Progress {
                phase: ProgressPhase::Writing,
                current_entry: Some(entry.relative_path.clone()),
                entries_done,
                entries_total,
                bytes_done,
                bytes_total,
            },
        )?;
        bytes_done = bytes_done.saturating_add(u64::from(rec.uncompressed));
        entries_done += 1;
        centrals.push(rec);
    }

    progress(Progress {
        phase: ProgressPhase::Finishing,
        current_entry: None,
        entries_done,
        entries_total,
        bytes_done,
        bytes_total,
    });

    let cd_offset = out.stream_position()?;
    if cd_offset > u32::MAX as u64 {
        return Err(ZipcoreError::Zip64Required {
            reason: "中央ディレクトリのオフセットが 4 GiB を超えています".into(),
        });
    }
    for rec in &centrals {
        write_central_header(&mut out, rec)?;
    }
    let cd_end = out.stream_position()?;
    let cd_size = cd_end - cd_offset;
    if cd_size > u32::MAX as u64 {
        return Err(ZipcoreError::Zip64Required {
            reason: "中央ディレクトリが 4 GiB を超えています".into(),
        });
    }
    write_eocd(
        &mut out,
        centrals.len() as u16,
        cd_size as u32,
        cd_offset as u32,
    )?;
    out.sync_all()?;

    Ok(WriteReport {
        dest: dest.to_path_buf(),
        entries_written: entries_done,
        bytes_written: bytes_done,
        excluded: plan.excluded.clone(),
    })
}

fn write_one_entry(
    out: &mut File,
    entry: &PlannedEntry,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Progress),
    snap: Progress,
) -> Result<CentralRecord, ZipcoreError> {
    if entry.name_raw.len() > u16::MAX as usize {
        return Err(ZipcoreError::NameTooLong {
            path: entry.relative_path.clone(),
        });
    }
    debug_assert_eq!(
        entry.name_raw,
        encode_entry_name(&entry.relative_path).cp932,
        "writer must persist encode_entry_name CP932 bytes"
    );

    let (dos_time, dos_date) = fs::metadata(&entry.source)
        .and_then(|m| m.modified())
        .map(crate::pkzip::msdos_datetime)
        .unwrap_or((0, (1 << 5) | 1));

    let local_offset = out.stream_position()?;
    if local_offset > u32::MAX as u64 {
        return Err(ZipcoreError::Zip64Required {
            reason: "ローカルヘッダのオフセットが 4 GiB を超えています".into(),
        });
    }

    if entry.is_dir {
        write_local_header(
            out,
            &entry.name_raw,
            METHOD_STORED,
            GPBF_PLAIN,
            dos_time,
            dos_date,
            0,
            0,
            0,
        )?;
        return Ok(CentralRecord {
            name: entry.name_raw.clone(),
            method: METHOD_STORED,
            gpbf: GPBF_PLAIN,
            crc: 0,
            compressed: 0,
            uncompressed: 0,
            local_offset: local_offset as u32,
            dos_time,
            dos_date,
            is_dir: true,
        });
    }

    write_local_header(
        out,
        &entry.name_raw,
        METHOD_DEFLATE,
        GPBF_PLAIN,
        dos_time,
        dos_date,
        0,
        0,
        0,
    )?;
    let data_start = out.stream_position()?;
    let mut hasher = crc32fast::Hasher::new();
    let mut uncompressed = 0u64;
    {
        let mut src = io::BufReader::new(File::open(&entry.source)?);
        let mut encoder = DeflateEncoder::new(&mut *out, Compression::new(DEFLATE_LEVEL));
        let mut buf = [0u8; COPY_BUF];
        loop {
            check_cancel(cancel)?;
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            uncompressed += n as u64;
            if uncompressed > u32::MAX as u64 {
                return Err(ZipcoreError::Zip64Required {
                    reason: format!("{} が 4 GiB 以上です", entry.relative_path),
                });
            }
            encoder.write_all(&buf[..n])?;
            let mut p = snap.clone();
            p.bytes_done = snap.bytes_done.saturating_add(uncompressed);
            progress(p);
        }
        encoder.finish()?;
    }
    let data_end = out.stream_position()?;
    let compressed = data_end - data_start;
    if compressed > u32::MAX as u64 {
        return Err(ZipcoreError::Zip64Required {
            reason: format!("{} の圧縮後サイズが 4 GiB 以上です", entry.relative_path),
        });
    }
    let crc = hasher.finalize();
    patch_local_sizes(
        out,
        local_offset,
        crc,
        compressed as u32,
        uncompressed as u32,
    )?;
    out.seek(SeekFrom::Start(data_end))?;

    Ok(CentralRecord {
        name: entry.name_raw.clone(),
        method: METHOD_DEFLATE,
        gpbf: GPBF_PLAIN,
        crc,
        compressed: compressed as u32,
        uncompressed: uncompressed as u32,
        local_offset: local_offset as u32,
        dos_time,
        dos_date,
        is_dir: false,
    })
}

struct RawItem {
    source: PathBuf,
    nfc_original: String,
    is_dir: bool,
    uncompressed_size: u64,
}

fn collect_item(
    item: &InputItem,
    out: &mut Vec<RawItem>,
    excluded: &mut Vec<String>,
) -> Result<(), ZipcoreError> {
    if !item.abs_path.exists() {
        return Err(ZipcoreError::InputMissing {
            path: item.abs_path.clone(),
        });
    }
    let meta = fs::symlink_metadata(&item.abs_path)?;
    if meta.file_type().is_symlink() {
        return Err(ZipcoreError::UnsupportedType {
            path: item.abs_path.clone(),
        });
    }
    let Some(base) = file_name_str(&item.abs_path) else {
        return Err(ZipcoreError::UnsupportedType {
            path: item.abs_path.clone(),
        });
    };
    if item.as_dir {
        if !meta.is_dir() {
            return Err(ZipcoreError::NotADirectory {
                path: item.abs_path.clone(),
            });
        }
        walk_dir(&item.abs_path, base, out, excluded)?;
    } else {
        if !meta.is_file() {
            return Err(ZipcoreError::NotAFile {
                path: item.abs_path.clone(),
            });
        }
        if is_junk(Path::new(base)) {
            excluded.push(base.to_string());
            return Ok(());
        }
        out.push(RawItem {
            source: item.abs_path.clone(),
            nfc_original: nfc_slash_path(base, false),
            is_dir: false,
            uncompressed_size: meta.len(),
        });
    }
    Ok(())
}

fn walk_dir(
    abs: &Path,
    rel: &str,
    out: &mut Vec<RawItem>,
    excluded: &mut Vec<String>,
) -> Result<(), ZipcoreError> {
    let nfc_rel = nfc_slash_path(rel, true);
    if is_junk(Path::new(nfc_rel.trim_end_matches('/'))) {
        excluded.push(nfc_rel.trim_end_matches('/').to_string());
        return Ok(());
    }
    out.push(RawItem {
        source: abs.to_path_buf(),
        nfc_original: nfc_rel,
        is_dir: true,
        uncompressed_size: 0,
    });

    let mut children: Vec<_> = fs::read_dir(abs)?.collect::<io::Result<Vec<_>>>()?;
    children.sort_by(|a, b| nfc_os(a.file_name()).cmp(&nfc_os(b.file_name())));
    for child in children {
        let path = child.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            return Err(ZipcoreError::UnsupportedType { path });
        }
        let Some(name) = file_name_str(&path) else {
            return Err(ZipcoreError::UnsupportedType { path });
        };
        let child_rel = format!("{}/{}", rel.trim_end_matches('/'), name);
        if meta.is_dir() {
            walk_dir(&path, &child_rel, out, excluded)?;
        } else if meta.is_file() {
            let nfc = nfc_slash_path(&child_rel, false);
            if is_junk(Path::new(&nfc)) {
                excluded.push(nfc);
                continue;
            }
            out.push(RawItem {
                source: path,
                nfc_original: nfc,
                is_dir: false,
                uncompressed_size: meta.len(),
            });
        } else {
            return Err(ZipcoreError::UnsupportedType { path });
        }
    }
    Ok(())
}

fn assign_names(
    collected: Vec<RawItem>,
    replacements: &BTreeMap<String, String>,
    excluded: Vec<String>,
) -> Result<Plan, ZipcoreError> {
    let mut used: HashSet<Vec<u8>> = HashSet::new();
    let mut entries = Vec::with_capacity(collected.len());
    let mut issues = Vec::new();
    let mut total_uncompressed = 0u64;

    for item in collected {
        let chosen = replacements
            .get(&item.nfc_original)
            .cloned()
            .unwrap_or_else(|| item.nfc_original.clone());
        let chosen = nfc_slash_path(&chosen, item.is_dir);
        let encoded = encode_entry_name(&chosen);
        issues.extend(encoded.issues.iter().cloned());
        let (relative_path, name_raw) = unique_cp932_name(&chosen, item.is_dir, &mut used)?;
        total_uncompressed = total_uncompressed.saturating_add(item.uncompressed_size);
        entries.push(PlannedEntry {
            source: item.source,
            relative_path,
            name_raw,
            is_dir: item.is_dir,
            uncompressed_size: item.uncompressed_size,
        });
    }

    Ok(Plan {
        entries,
        issues,
        excluded,
        total_uncompressed,
    })
}

fn unique_cp932_name(
    unicode: &str,
    is_dir: bool,
    used: &mut HashSet<Vec<u8>>,
) -> Result<(String, Vec<u8>), ZipcoreError> {
    let base = nfc_slash_path(unicode, is_dir);
    for n in 1..=MAX_COPY_SUFFIX {
        let candidate = if n == 1 {
            base.clone()
        } else {
            with_copy_suffix(&base, n)
        };
        let raw = encode_entry_name(&candidate).cp932;
        if used.insert(raw.clone()) {
            return Ok((candidate, raw));
        }
    }
    Err(ZipcoreError::CollisionLimit { path: base })
}

fn with_copy_suffix(name: &str, n: u32) -> String {
    let trailing = name.ends_with('/');
    let trimmed = name.trim_end_matches('/');
    let (parent, base) = match trimmed.rfind('/') {
        Some(i) => (&trimmed[..i], &trimmed[i + 1..]),
        None => ("", trimmed),
    };
    let (stem, ext) = split_stem_ext(base);
    let new_base = match ext {
        Some(ext) => format!("{stem} ({n}).{ext}"),
        None => format!("{stem} ({n})"),
    };
    let mut out = if parent.is_empty() {
        new_base
    } else {
        format!("{parent}/{new_base}")
    };
    if trailing {
        out.push('/');
    }
    out
}

fn split_stem_ext(base: &str) -> (String, Option<String>) {
    let dots = base.chars().filter(|c| *c == '.').count();
    if base.starts_with('.') && dots == 1 {
        return (base.to_string(), None);
    }
    match base.rfind('.') {
        Some(i) if i > 0 => (base[..i].to_string(), Some(base[i + 1..].to_string())),
        _ => (base.to_string(), None),
    }
}

fn nfc_slash_path(unicode_path: &str, force_trailing_slash: bool) -> String {
    let slash_path = unicode_path.replace('\\', "/");
    let trailing = force_trailing_slash || slash_path.ends_with('/');
    let comps: Vec<String> = slash_path
        .split('/')
        .filter(|c| !c.is_empty())
        .map(|c| c.nfc().collect())
        .collect();
    let mut s = comps.join("/");
    if trailing && !s.ends_with('/') {
        s.push('/');
    }
    s
}

fn file_name_str(path: &Path) -> Option<&str> {
    path.file_name().and_then(|n| n.to_str())
}

fn nfc_os(name: impl Into<PathBuf>) -> String {
    name.into().to_string_lossy().nfc().collect()
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), ZipcoreError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ZipcoreError::Cancelled)
    } else {
        Ok(())
    }
}

fn dest_overlaps_input(dest: &Path, inputs: &[InputItem]) -> bool {
    let dest = absolute(dest);
    inputs.iter().any(|item| {
        let input = absolute(&item.abs_path);
        dest == input || (item.as_dir && dest.starts_with(&input))
    })
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
