//! ZIP 作成（任意 AES-256 AE-2、任意分割）。ファイル名は `encode_entry_name` の CP932 バイトのみ。
//! 分割は出力ストリームの差し替えと、EOCD/CD のディスク番号だけ。非分割のヘッダ契約は変えない。

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::aeswrite::{
    aes_extra_field, Aes256Writer, GPBF_ENCRYPT, METHOD_AES, VERSION_NEEDED_AES,
};
use crate::encoding::{encode_entry_name, is_junk, Unmappable};
use crate::error::ZipcoreError;
use crate::fsutil::{atomic_replace, path_eq, resolve_existing, resolve_path, stays_under};
use crate::job::check_cancel;
use crate::pkzip::{
    inspect_reader, inspect_zip, patch_local_sizes, write_central_header, write_eocd,
    write_local_header, CentralRecord, GPBF_PLAIN, METHOD_DEFLATE, METHOD_STORED, VERSION_NEEDED,
};
use crate::split::{self, SplitSink, SplitWriter};

pub use crate::job::{Progress, ProgressPhase, SecretString};
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WriteReport {
    pub dest: PathBuf,
    pub entries_written: u32,
    pub bytes_written: u64,
    pub excluded: Vec<String>,
}

pub fn inspect_zip_headers(path: &Path) -> Result<Vec<InspectedEntry>, ZipcoreError> {
    let volumes = split::discover_volumes(path);
    if volumes.len() <= 1 {
        inspect_zip(path)
    } else {
        let data = split::linearize_volumes(path)?;
        inspect_reader(&mut io::Cursor::new(data))
    }
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
    if let Some(ref password) = req.password {
        if password.is_empty() {
            return Err(ZipcoreError::EmptyPassword);
        }
    }
    let _ = req.aes;
    if let Some(0) = req.split_bytes {
        return Err(ZipcoreError::InvalidSplit {
            reason: "0 バイトには分割できません".into(),
        });
    }

    let plan = plan_write(&req)?;
    if req.password.is_some() && plan.entries.iter().all(|e| e.is_dir) {
        return Err(ZipcoreError::NoEncryptableFiles);
    }
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
    if dest_overlaps_input(&req.dest, &req.inputs, &plan) {
        return Err(ZipcoreError::DestOverlapsInput {
            dest: req.dest.clone(),
        });
    }

    let dest = req.dest.clone();
    let password = req.password.as_ref().map(SecretString::expose_bytes);
    let report = write_plan_to_path(
        &plan,
        &dest,
        password,
        req.split_bytes,
        &mut progress,
        cancel,
    )?;
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

fn write_plan_to_path(
    plan: &Plan,
    dest: &Path,
    password: Option<&[u8]>,
    split_bytes: Option<u64>,
    progress: &mut impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<WriteReport, ZipcoreError> {
    if let Some(n) = split_bytes {
        return write_plan_split(plan, dest, password, n, progress, cancel);
    }
    let partial = partial_path(dest);
    let result = write_plan_to_partial(plan, dest, &partial, password, progress, cancel);
    match result {
        Ok(report) => {
            if let Err(e) = replace_file(&partial, dest) {
                let _ = fs::remove_file(&partial);
                return Err(e.into());
            }
            let _ = split::discard_stale_volumes(dest, 1);
            Ok(report)
        }
        Err(e) => {
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn write_plan_split(
    plan: &Plan,
    dest: &Path,
    password: Option<&[u8]>,
    split_bytes: u64,
    progress: &mut impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<WriteReport, ZipcoreError> {
    let mut out = SplitWriter::create(dest.to_path_buf(), split_bytes)?;
    match write_plan_into(&mut out, plan, dest, password, progress, cancel) {
        Ok(report) => {
            let pieces = out.finish()?;
            let _ = split::discard_stale_volumes(dest, pieces.len() as u32);
            Ok(report)
        }
        Err(e) => {
            out.abort();
            Err(map_split_write_err(e))
        }
    }
}

fn map_split_write_err(err: ZipcoreError) -> ZipcoreError {
    match err {
        ZipcoreError::Io(io) => split::map_split_io(io),
        other => other,
    }
}

fn write_plan_to_partial(
    plan: &Plan,
    dest: &Path,
    partial: &Path,
    password: Option<&[u8]>,
    progress: &mut impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<WriteReport, ZipcoreError> {
    let mut out = File::create(partial)?;
    let report = write_plan_into(&mut out, plan, dest, password, progress, cancel)?;
    out.sync_all()?;
    Ok(report)
}

fn write_plan_into<W: SplitSink>(
    out: &mut W,
    plan: &Plan,
    dest: &Path,
    password: Option<&[u8]>,
    progress: &mut impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<WriteReport, ZipcoreError> {
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
            out,
            entry,
            password,
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

    out.finish_on_last_piece();
    let cd_disk = out.volume_index();
    let cd_rel = out.offset_in_volume()?;
    if cd_rel > u32::MAX as u64 {
        return Err(ZipcoreError::Zip64Required {
            reason: "中央ディレクトリのオフセットが 4 GiB を超えています".into(),
        });
    }
    let cd_abs = out.stream_position()?;
    for rec in &centrals {
        write_central_header(out, rec)?;
    }
    let cd_end = out.stream_position()?;
    let cd_size = cd_end - cd_abs;
    if cd_size > u32::MAX as u64 {
        return Err(ZipcoreError::Zip64Required {
            reason: "中央ディレクトリが 4 GiB を超えています".into(),
        });
    }
    let this_disk = out.volume_index();
    write_eocd(
        out,
        centrals.len() as u16,
        cd_size as u32,
        cd_rel as u32,
        this_disk,
        cd_disk,
    )?;

    Ok(WriteReport {
        dest: dest.to_path_buf(),
        entries_written: entries_done,
        bytes_written: bytes_done,
        excluded: plan.excluded.clone(),
    })
}

fn write_one_entry<W: SplitSink>(
    out: &mut W,
    entry: &PlannedEntry,
    password: Option<&[u8]>,
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

    let extra_len = if password.is_some() && !entry.is_dir {
        aes_extra_field(METHOD_DEFLATE).len()
    } else {
        0
    };
    let header_len = 30u64 + entry.name_raw.len() as u64 + extra_len as u64;
    out.ensure_record_fits(header_len)?;

    let local_abs = out.stream_position()?;
    let disk_start = out.volume_index();
    let local_offset = out.offset_in_volume()?;
    if local_offset > u32::MAX as u64 {
        return Err(ZipcoreError::Zip64Required {
            reason: "ローカルヘッダのオフセットが 4 GiB を超えています".into(),
        });
    }

    if entry.is_dir {
        write_local_header(
            out,
            &entry.name_raw,
            &[],
            METHOD_STORED,
            GPBF_PLAIN,
            dos_time,
            dos_date,
            0,
            0,
            0,
            VERSION_NEEDED,
        )?;
        return Ok(CentralRecord {
            name: entry.name_raw.clone(),
            extra: Vec::new(),
            method: METHOD_STORED,
            gpbf: GPBF_PLAIN,
            crc: 0,
            compressed: 0,
            uncompressed: 0,
            local_offset: local_offset as u32,
            disk_start,
            dos_time,
            dos_date,
            version_needed: VERSION_NEEDED,
            is_dir: true,
        });
    }

    let encrypt = password.is_some();
    let extra = if encrypt {
        aes_extra_field(METHOD_DEFLATE)
    } else {
        Vec::new()
    };
    let (method, gpbf, version_needed) = if encrypt {
        (METHOD_AES, GPBF_ENCRYPT, VERSION_NEEDED_AES)
    } else {
        (METHOD_DEFLATE, GPBF_PLAIN, VERSION_NEEDED)
    };

    write_local_header(
        out,
        &entry.name_raw,
        &extra,
        method,
        gpbf,
        dos_time,
        dos_date,
        0,
        0,
        0,
        version_needed,
    )?;
    let data_start = out.stream_position()?;
    let mut hasher = crc32fast::Hasher::new();
    let mut uncompressed = 0u64;
    {
        let mut src = io::BufReader::new(File::open(&entry.source)?);
        let mut copy_plain = |sink: &mut dyn Write| -> Result<(), ZipcoreError> {
            let mut encoder = DeflateEncoder::new(sink, Compression::new(DEFLATE_LEVEL));
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
            Ok(())
        };
        if let Some(password) = password {
            let mut aes = Aes256Writer::new(&mut *out, password)?;
            copy_plain(&mut aes)?;
            aes.finish()?;
        } else {
            copy_plain(&mut *out)?;
        }
    }
    let data_end = out.stream_position()?;
    let compressed = data_end - data_start;
    if compressed > u32::MAX as u64 {
        return Err(ZipcoreError::Zip64Required {
            reason: format!("{} の圧縮後サイズが 4 GiB 以上です", entry.relative_path),
        });
    }
    let crc = if encrypt { 0 } else { hasher.finalize() };
    patch_local_sizes(out, local_abs, crc, compressed as u32, uncompressed as u32)?;
    out.seek(SeekFrom::Start(data_end))?;

    Ok(CentralRecord {
        name: entry.name_raw.clone(),
        extra,
        method,
        gpbf,
        crc,
        compressed: compressed as u32,
        uncompressed: uncompressed as u32,
        local_offset: local_offset as u32,
        disk_start,
        dos_time,
        dos_date,
        version_needed,
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

fn dest_overlaps_input(dest: &Path, inputs: &[InputItem], plan: &Plan) -> bool {
    let dest = resolve_path(dest);
    inputs.iter().any(|item| {
        let input = resolve_existing(&item.abs_path);
        path_eq(&dest, &input) || (item.as_dir && stays_under(&dest, &input))
    }) || plan
        .entries
        .iter()
        .any(|e| path_eq(&dest, &resolve_existing(&e.source)))
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(".partial");
    PathBuf::from(name)
}

fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    match atomic_replace(from, to)? {
        Some(backup) => {
            let _ = fs::remove_file(&backup);
            Ok(())
        }
        None => Ok(()),
    }
}
