//! WinZip 形式の分割出力。ZIP の中身（エンコーディング / ヘッダ）は変えない。
//!
//! ピース名は `{stem}.z01` … 最後が `{stem}.zip`（`dest`）。
//! 非末尾ピースは `split_bytes` を超えない。キャンセル時は部分ピースを全部消す。

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::ZipcoreError;
use crate::fsutil::{path_eq, replace_keeping_backup};
use crate::pkzip::SIG_EOCD;

/// ピース数の上限。`.z01` … `.z998` + 最後の `.zip`。
pub const MAX_PIECES: u32 = 999;

const TOO_MANY: &str = "ZIP_SPLIT_TOO_MANY";

/// `dest`（最後の `.zip`）から stem を取る。
pub fn zip_stem(dest: &Path) -> PathBuf {
    match dest.file_stem() {
        Some(s) if !s.is_empty() => PathBuf::from(s),
        _ => dest
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("archive")),
    }
}

fn zip_parent(dest: &Path) -> PathBuf {
    dest.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 1 始まりの非末尾ピース（`.z01`）または末尾（`dest`）。
pub fn volume_path(dest: &Path, index_1: u32, is_last: bool) -> PathBuf {
    if is_last {
        return dest.to_path_buf();
    }
    let mut name = zip_stem(dest).into_os_string();
    name.push(format!(".z{index_1:02}"));
    zip_parent(dest).join(name)
}

fn partial_path_for(dest: &Path, index_1: u32) -> PathBuf {
    let mut name = zip_stem(dest).into_os_string();
    name.push(format!(".z{index_1:02}.partial"));
    zip_parent(dest).join(name)
}

fn is_z_digits(rest: &str) -> bool {
    let rest = rest.as_bytes();
    !rest.is_empty() && rest.iter().all(|b| b.is_ascii_digit())
}

/// 保存先 ZIP と同じ場所・同じ stem の `.zNN` ピースか。親フォルダは開かない。
pub fn is_split_sibling(dest_zip: &Path, candidate: &Path) -> bool {
    let dest_parent = zip_parent(dest_zip);
    let cand_parent = candidate
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    if !path_eq(&dest_parent, &cand_parent) {
        return false;
    }
    let Some(cand_name) = candidate.file_name() else {
        return false;
    };
    let stem = zip_stem(dest_zip);
    let Some(stem_str) = stem.to_str() else {
        return false;
    };
    let Some(name) = cand_name.to_str() else {
        return false;
    };
    let prefix = format!("{stem_str}.");
    let Some(ext) = name.strip_prefix(&prefix) else {
        return false;
    };
    let ext = ext.to_ascii_lowercase();
    ext.starts_with('z') && is_z_digits(&ext[1..])
}

/// `foo.zip` の隣に `foo.z01` があれば分割セット。順序は z01, z02, …, zip。
pub fn discover_volumes(zip_path: &Path) -> Vec<PathBuf> {
    let first = volume_path(zip_path, 1, false);
    if !first.exists() {
        return vec![zip_path.to_path_buf()];
    }
    let mut out = Vec::new();
    let mut i = 1u32;
    while i <= MAX_PIECES {
        let p = volume_path(zip_path, i, false);
        if !p.exists() {
            break;
        }
        out.push(p);
        i += 1;
    }
    out.push(zip_path.to_path_buf());
    out
}

/// 完成後の最終パス一覧（非末尾 `.zNN` + 最後の `dest`）。
pub fn final_volume_paths(dest: &Path, count: u32) -> Vec<PathBuf> {
    if count <= 1 {
        return vec![dest.to_path_buf()];
    }
    let mut out = Vec::with_capacity(count as usize);
    for i in 1..count {
        out.push(volume_path(dest, i, false));
    }
    out.push(dest.to_path_buf());
    out
}

pub fn discard_paths(paths: &[PathBuf]) {
    for p in paths {
        let _ = fs::remove_file(p);
    }
}

/// `dest` 向けに残った `.zNN.partial` を消す。欠番があっても全部見る。
pub fn discard_partials_for(dest: &Path) {
    for i in 1..=MAX_PIECES {
        let _ = fs::remove_file(partial_path_for(dest, i));
    }
}

/// 分割セットを 1 本の ZIP バイト列に連結する（検査・zip crate 展開用）。
pub fn join_volumes(zip_path: &Path) -> io::Result<Vec<u8>> {
    let volumes = discover_volumes(zip_path);
    let mut buf = Vec::new();
    for p in volumes {
        buf.extend_from_slice(&fs::read(p)?);
    }
    Ok(buf)
}

pub fn join_volumes_to(zip_path: &Path, dest: &Path) -> io::Result<()> {
    let bytes = linearize_volumes(zip_path)?;
    fs::write(dest, bytes)
}

/// 分割セットを単一 ZIP バイト列にする。マルチディスクならオフセットを連結座標へ直す。
pub fn linearize_volumes(zip_path: &Path) -> io::Result<Vec<u8>> {
    let volumes = discover_volumes(zip_path);
    if volumes.len() <= 1 {
        return fs::read(zip_path);
    }
    let mut sizes = Vec::with_capacity(volumes.len());
    let mut data = Vec::new();
    for p in &volumes {
        let chunk = fs::read(p)?;
        sizes.push(chunk.len());
        data.extend_from_slice(&chunk);
    }
    linearize_concatenated(&mut data, &sizes)?;
    Ok(data)
}

fn disk_prefix(sizes: &[usize], disk: u16) -> io::Result<u64> {
    let idx = disk as usize;
    if idx >= sizes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "分割 ZIP のディスク番号がピース数を超えています",
        ));
    }
    Ok(sizes.iter().take(idx).map(|n| *n as u64).sum())
}

fn linearize_concatenated(data: &mut [u8], sizes: &[usize]) -> io::Result<()> {
    let Some(eocd) = find_eocd_end(data) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "EOCD が見つかりません",
        ));
    };
    let this_disk = u16::from_le_bytes(data[eocd + 4..eocd + 6].try_into().unwrap());
    let cd_disk = u16::from_le_bytes(data[eocd + 6..eocd + 8].try_into().unwrap());
    if this_disk == 0 && cd_disk == 0 {
        return Ok(());
    }
    let cd_entries = u16::from_le_bytes(data[eocd + 10..eocd + 12].try_into().unwrap()) as usize;
    let cd_size = u32::from_le_bytes(data[eocd + 12..eocd + 16].try_into().unwrap()) as usize;
    let cd_rel = u32::from_le_bytes(data[eocd + 16..eocd + 20].try_into().unwrap()) as u64;
    let cd_abs = disk_prefix(sizes, cd_disk)? + cd_rel;
    let mut i = cd_abs as usize;
    for _ in 0..cd_entries {
        if i + 46 > data.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "中央ディレクトリが途中で切れています",
            ));
        }
        let disk_start = u16::from_le_bytes(data[i + 34..i + 36].try_into().unwrap());
        let local_rel = u32::from_le_bytes(data[i + 42..i + 46].try_into().unwrap()) as u64;
        let local_abs = disk_prefix(sizes, disk_start)? + local_rel;
        if local_abs > u32::MAX as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "連結後のローカルオフセットが 4 GiB を超えています",
            ));
        }
        data[i + 34..i + 36].copy_from_slice(&0u16.to_le_bytes());
        data[i + 42..i + 46].copy_from_slice(&(local_abs as u32).to_le_bytes());
        let name_len = u16::from_le_bytes(data[i + 28..i + 30].try_into().unwrap()) as usize;
        let extra_len = u16::from_le_bytes(data[i + 30..i + 32].try_into().unwrap()) as usize;
        let comment_len = u16::from_le_bytes(data[i + 32..i + 34].try_into().unwrap()) as usize;
        i += 46 + name_len + extra_len + comment_len;
    }
    if i != cd_abs as usize + cd_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "中央ディレクトリのサイズが合いません",
        ));
    }
    if cd_abs > u32::MAX as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "連結後の中央ディレクトリオフセットが 4 GiB を超えています",
        ));
    }
    data[eocd + 4..eocd + 6].copy_from_slice(&0u16.to_le_bytes());
    data[eocd + 6..eocd + 8].copy_from_slice(&0u16.to_le_bytes());
    data[eocd + 16..eocd + 20].copy_from_slice(&(cd_abs as u32).to_le_bytes());
    Ok(())
}

fn find_eocd_end(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    let max_back = (data.len() - 22).min(65_535);
    let start = data.len() - 22 - max_back;
    let buf = &data[start..];
    let last = buf.len() - 22;
    for i in (0..=last).rev() {
        let sig = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap());
        if sig != SIG_EOCD {
            continue;
        }
        let comment_len = u16::from_le_bytes(buf[i + 20..i + 22].try_into().unwrap()) as usize;
        if i + 22 + comment_len == buf.len() {
            return Some(start + i);
        }
    }
    None
}

/// 最後の `.zip` ピースの EOCD（this_disk, cd_disk, cd_size, cd_offset）。
pub fn last_volume_eocd(zip_path: &Path) -> io::Result<(u16, u16, u32, u32)> {
    let volumes = discover_volumes(zip_path);
    let last = volumes
        .last()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "分割ピースがありません"))?;
    let data = fs::read(last)?;
    let eocd = find_eocd_end(&data).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "最後の .zip に EOCD がありません",
        )
    })?;
    Ok((
        u16::from_le_bytes(data[eocd + 4..eocd + 6].try_into().unwrap()),
        u16::from_le_bytes(data[eocd + 6..eocd + 8].try_into().unwrap()),
        u32::from_le_bytes(data[eocd + 12..eocd + 16].try_into().unwrap()),
        u32::from_le_bytes(data[eocd + 16..eocd + 20].try_into().unwrap()),
    ))
}

/// 連結ストリームとして分割ピースを読む。
pub struct ConcatReader {
    files: Vec<File>,
    sizes: Vec<u64>,
    pos: u64,
    total: u64,
}

impl ConcatReader {
    pub fn open(volumes: &[PathBuf]) -> io::Result<Self> {
        let mut files = Vec::with_capacity(volumes.len());
        let mut sizes = Vec::with_capacity(volumes.len());
        let mut total = 0u64;
        for p in volumes {
            let f = File::open(p)?;
            let len = f.metadata()?.len();
            files.push(f);
            sizes.push(len);
            total = total.saturating_add(len);
        }
        Ok(Self {
            files,
            sizes,
            pos: 0,
            total,
        })
    }

    fn locate(&mut self) -> io::Result<(usize, u64)> {
        let mut remain = self.pos;
        for (i, size) in self.sizes.iter().copied().enumerate() {
            if remain < size || i + 1 == self.sizes.len() {
                self.files[i].seek(SeekFrom::Start(remain))?;
                return Ok((i, remain));
            }
            remain -= size;
        }
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "分割ピースの位置が範囲外です",
        ))
    }
}

impl Read for ConcatReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut last_idx = None;
        while self.pos < self.total {
            let (idx, _) = self.locate()?;
            let n = self.files[idx].read(buf)?;
            if n > 0 {
                self.pos += n as u64;
                return Ok(n);
            }
            if last_idx == Some(idx) {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "分割ピースが途中で終わりました",
                ));
            }
            last_idx = Some(idx);
            let piece_end: u64 = self.sizes.iter().take(idx + 1).copied().sum();
            if self.pos < piece_end {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "分割ピースが途中で終わりました",
                ));
            }
            if idx + 1 >= self.files.len() {
                break;
            }
        }
        Ok(0)
    }
}

impl Seek for ConcatReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(p) => p,
            SeekFrom::End(p) => self.total.saturating_add_signed(p),
            SeekFrom::Current(p) => self.pos.saturating_add_signed(p),
        };
        if next > self.total {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "分割ピースの末尾を超えています",
            ));
        }
        self.pos = next;
        Ok(self.pos)
    }
}

/// 分割時は最後のピースに CD/EOCD を載せ、ディスク番号とディスク相対オフセットを書く。
///
/// 7-Zip（ZipIn.cpp `ReadVols`）は EOCD の `ThisDisk == 0` だと `.z01` を探さず、
/// 最後の `.zip` だけを単体 ZIP として開く。非分割の `File` は disk = 0 のまま。
pub trait SplitSink: Write + Seek {
    fn finish_on_last_piece(&mut self) {}
    fn volume_index(&self) -> u16 {
        0
    }
    fn offset_in_volume(&mut self) -> io::Result<u64> {
        self.stream_position()
    }
    /// ローカルヘッダがピース境界をまたがないようにする。7-Zip はヘッダ跨ぎを Headers Error にする。
    fn ensure_record_fits(&mut self, _len: u64) -> io::Result<()> {
        Ok(())
    }
}

impl SplitSink for File {}

/// 論理オフセットをピースへ割り振る `Write + Seek`。
pub struct SplitWriter {
    dest: PathBuf,
    split_bytes: u64,
    files: Vec<File>,
    paths: Vec<PathBuf>,
    pos: u64,
    len: u64,
    unbounded_last: bool,
}

impl SplitWriter {
    pub fn create(dest: PathBuf, split_bytes: u64) -> Result<Self, ZipcoreError> {
        if split_bytes == 0 {
            return Err(ZipcoreError::InvalidSplit {
                reason: "0 バイトには分割できません".into(),
            });
        }
        let mut writer = Self {
            dest,
            split_bytes,
            files: Vec::new(),
            paths: Vec::new(),
            pos: 0,
            len: 0,
            unbounded_last: false,
        };
        writer.ensure_piece(0).map_err(map_split_io)?;
        Ok(writer)
    }

    pub fn piece_count(&self) -> u32 {
        self.paths.len() as u32
    }

    pub fn logical_len(&self) -> u64 {
        self.len
    }

    pub fn split_size(&self) -> u64 {
        self.split_bytes
    }

    fn last_piece_start(&self) -> u64 {
        self.files.len().saturating_sub(1) as u64 * self.split_bytes
    }

    fn ensure_piece(&mut self, index: usize) -> io::Result<()> {
        if index >= MAX_PIECES as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, TOO_MANY));
        }
        while self.files.len() <= index {
            let n = self.files.len() as u32 + 1;
            let path = partial_path_for(&self.dest, n);
            let file = File::create(&path)?;
            self.paths.push(path);
            self.files.push(file);
        }
        Ok(())
    }

    fn locate(&mut self) -> io::Result<()> {
        let idx = (self.pos / self.split_bytes) as usize;
        let off = self.pos % self.split_bytes;
        self.ensure_piece(idx)?;
        self.files[idx].seek(SeekFrom::Start(off))?;
        Ok(())
    }

    pub fn abort(&mut self) {
        for f in &mut self.files {
            let _ = f.sync_all();
        }
        self.files.clear();
        discard_paths(&self.paths);
        self.paths.clear();
    }

    /// 部分ファイルを最終名へ置き、最終ピースパスを返す。
    pub fn finish(mut self) -> Result<Vec<PathBuf>, ZipcoreError> {
        for f in &mut self.files {
            f.sync_all().map_err(map_split_io)?;
        }
        self.files.clear();
        let partials = std::mem::take(&mut self.paths);
        let count = partials.len() as u32;
        if count == 0 {
            return Err(ZipcoreError::InvalidSplit {
                reason: "ピースがありません".into(),
            });
        }
        let finals = final_volume_paths(&self.dest, count);
        let mut placed: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
        for (partial, final_path) in partials.iter().zip(finals.iter()) {
            match replace_keeping_backup(partial, final_path) {
                Ok(backup) => placed.push((final_path.clone(), backup)),
                Err(e) => {
                    discard_paths(&partials);
                    for (p, backup) in placed.into_iter().rev() {
                        if let Some(bak) = backup {
                            if fs::rename(&bak, &p).is_err() {
                                if fs::copy(&bak, &p).is_ok() {
                                    let _ = fs::remove_file(&bak);
                                }
                            }
                        } else {
                            let _ = fs::remove_file(&p);
                        }
                    }
                    return Err(e.into());
                }
            }
        }
        for (_, backup) in &placed {
            if let Some(bak) = backup {
                let _ = fs::remove_file(bak);
            }
        }
        Ok(finals)
    }
}

pub fn map_split_io(err: io::Error) -> ZipcoreError {
    if err.kind() == io::ErrorKind::InvalidInput && err.to_string().contains(TOO_MANY) {
        ZipcoreError::LimitsExceeded {
            reason: "分割の個数が上限を超えました".into(),
        }
    } else {
        err.into()
    }
}

impl SplitSink for SplitWriter {
    fn finish_on_last_piece(&mut self) {
        self.unbounded_last = true;
    }

    fn volume_index(&self) -> u16 {
        if self.files.is_empty() {
            return 0;
        }
        if self.unbounded_last && self.pos >= self.last_piece_start() {
            return (self.files.len() - 1) as u16;
        }
        (self.pos / self.split_bytes) as u16
    }

    fn offset_in_volume(&mut self) -> io::Result<u64> {
        if self.unbounded_last && !self.files.is_empty() && self.pos >= self.last_piece_start() {
            return Ok(self.pos - self.last_piece_start());
        }
        Ok(self.pos % self.split_bytes)
    }

    fn ensure_record_fits(&mut self, len: u64) -> io::Result<()> {
        if self.unbounded_last || len == 0 || len >= self.split_bytes {
            return Ok(());
        }
        let off = self.pos % self.split_bytes;
        if off == 0 {
            return Ok(());
        }
        let room = self.split_bytes - off;
        if room >= len {
            return Ok(());
        }
        let pad = vec![0u8; room as usize];
        self.write_all(&pad)
    }
}

impl Write for SplitWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.unbounded_last && !self.files.is_empty() && self.pos >= self.last_piece_start() {
            let idx = self.files.len() - 1;
            let off = self.pos - self.last_piece_start();
            self.files[idx].seek(SeekFrom::Start(off))?;
            let wrote = self.files[idx].write(buf)?;
            self.pos += wrote as u64;
            self.len = self.len.max(self.pos);
            return Ok(wrote);
        }
        self.locate()?;
        let piece_off = self.pos % self.split_bytes;
        let room = self.split_bytes - piece_off;
        let n = buf.len().min(room as usize);
        let idx = (self.pos / self.split_bytes) as usize;
        let wrote = self.files[idx].write(&buf[..n])?;
        self.pos += wrote as u64;
        self.len = self.len.max(self.pos);
        Ok(wrote)
    }

    fn flush(&mut self) -> io::Result<()> {
        for f in &mut self.files {
            f.flush()?;
        }
        Ok(())
    }
}

impl Seek for SplitWriter {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(p) => p,
            SeekFrom::End(p) => self.len.saturating_add_signed(p),
            SeekFrom::Current(p) => self.pos.saturating_add_signed(p),
        };
        self.pos = next;
        if self.files.is_empty() {
            return Ok(self.pos);
        }
        if self.unbounded_last && self.pos >= self.last_piece_start() {
            let off = self.pos - self.last_piece_start();
            let idx = self.files.len() - 1;
            self.files[idx].seek(SeekFrom::Start(off))?;
            return Ok(self.pos);
        }
        if self.pos <= self.len {
            self.locate()?;
        }
        Ok(self.pos)
    }
}

impl Drop for SplitWriter {
    fn drop(&mut self) {
        if !self.paths.is_empty() {
            self.abort();
        }
    }
}

/// 最後の `.zip` ファイルの末尾が EOCD か（コメントなしなら末尾 22 バイト）。
pub fn eocd_in_last_volume(zip_path: &Path) -> io::Result<bool> {
    let volumes = discover_volumes(zip_path);
    let last = volumes
        .last()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "分割ピースがありません"))?;
    let data = fs::read(last)?;
    Ok(find_eocd_end(&data).is_some())
}

pub fn piece_sizes(zip_path: &Path) -> io::Result<Vec<u64>> {
    discover_volumes(zip_path)
        .into_iter()
        .map(|p| fs::metadata(p).map(|m| m.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::tests_support::TempDir;
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};

    #[test]
    fn volume_names_are_z01_then_zip() {
        let dest = Path::new("/tmp/送り.zip");
        assert_eq!(volume_path(dest, 1, false), PathBuf::from("/tmp/送り.z01"));
        assert_eq!(volume_path(dest, 2, false), PathBuf::from("/tmp/送り.z02"));
        assert_eq!(volume_path(dest, 3, true), PathBuf::from("/tmp/送り.zip"));
        assert_eq!(
            final_volume_paths(dest, 3),
            vec![
                PathBuf::from("/tmp/送り.z01"),
                PathBuf::from("/tmp/送り.z02"),
                PathBuf::from("/tmp/送り.zip"),
            ]
        );
        assert_eq!(
            final_volume_paths(dest, 1),
            vec![PathBuf::from("/tmp/送り.zip")]
        );
    }

    #[test]
    fn sibling_allows_z_digits_only() {
        let dest = Path::new("/tmp/out.zip");
        assert!(is_split_sibling(dest, Path::new("/tmp/out.z01")));
        assert!(is_split_sibling(dest, Path::new("/tmp/out.z12")));
        assert!(!is_split_sibling(dest, dest));
        assert!(!is_split_sibling(dest, Path::new("/tmp/secret.txt")));
        assert!(!is_split_sibling(dest, Path::new("/tmp/out.txt")));
        assert!(!is_split_sibling(dest, Path::new("/tmp/out.z01.exe")));
        assert!(!is_split_sibling(dest, Path::new("/other/out.z01")));
        assert!(!is_split_sibling(dest, Path::new("/tmp/other.z01")));
    }

    #[test]
    fn writer_rolls_and_seek_patches_earlier_piece() {
        let tmp = TempDir::new();
        let dest = tmp.path().join("out.zip");
        let mut w = SplitWriter::create(dest.clone(), 16).unwrap();
        w.write_all(&[1u8; 40]).unwrap();
        w.seek(SeekFrom::Start(2)).unwrap();
        w.write_all(&[9, 9]).unwrap();
        w.seek(SeekFrom::Start(40)).unwrap();
        let pieces = w.finish().unwrap();
        assert_eq!(pieces.len(), 3);
        assert!(pieces[0].ends_with("out.z01"));
        assert!(pieces[1].ends_with("out.z02"));
        assert_eq!(pieces[2], dest);
        assert_eq!(fs::metadata(&pieces[0]).unwrap().len(), 16);
        assert_eq!(fs::metadata(&pieces[1]).unwrap().len(), 16);
        assert_eq!(fs::metadata(&pieces[2]).unwrap().len(), 8);
        let joined = join_volumes(&dest).unwrap();
        assert_eq!(joined.len(), 40);
        assert_eq!(&joined[2..4], &[9, 9]);
        assert_eq!(joined[0], 1);
        let mut concat = ConcatReader::open(&pieces).unwrap();
        let mut got = Vec::new();
        concat.read_to_end(&mut got).unwrap();
        assert_eq!(got, joined);
    }

    #[test]
    fn finish_on_last_piece_keeps_eocd_together() {
        let tmp = TempDir::new();
        let dest = tmp.path().join("out.zip");
        let mut w = SplitWriter::create(dest.clone(), 16).unwrap();
        w.write_all(&[1u8; 20]).unwrap();
        w.finish_on_last_piece();
        let mut eocd = [0u8; 22];
        eocd[0..4].copy_from_slice(&SIG_EOCD.to_le_bytes());
        w.write_all(&eocd).unwrap();
        let pieces = w.finish().unwrap();
        assert!(pieces.len() >= 2);
        let last = fs::read(&dest).unwrap();
        assert!(last.len() >= 22);
        assert_eq!(
            &last[last.len() - 22..last.len() - 18],
            &SIG_EOCD.to_le_bytes()
        );
        assert!(eocd_in_last_volume(&dest).unwrap());
        let z01 = fs::metadata(&pieces[0]).unwrap().len();
        assert!(z01 <= 16, "non-last piece {z01} > 16");
        assert_eq!(pieces.len() - 1, 1);
    }

    #[test]
    fn ensure_record_fits_starts_header_on_next_piece() {
        let tmp = TempDir::new();
        let dest = tmp.path().join("out.zip");
        let mut w = SplitWriter::create(dest, 16).unwrap();
        w.write_all(&[1u8; 10]).unwrap();
        w.ensure_record_fits(8).unwrap();
        assert_eq!(w.volume_index(), 1);
        assert_eq!(w.offset_in_volume().unwrap(), 0);
        w.write_all(&[9u8; 8]).unwrap();
        let pieces = w.finish().unwrap();
        assert_eq!(fs::metadata(&pieces[0]).unwrap().len(), 16);
        let first = fs::read(&pieces[0]).unwrap();
        assert_eq!(&first[..10], &[1u8; 10]);
        assert_eq!(&first[10..], &[0u8; 6]);
        assert_eq!(fs::read(&pieces[1]).unwrap(), vec![9u8; 8]);
    }

    #[test]
    fn unbounded_volume_index_stays_on_last_piece() {
        let tmp = TempDir::new();
        let dest = tmp.path().join("out.zip");
        let mut w = SplitWriter::create(dest, 16).unwrap();
        w.write_all(&[1u8; 20]).unwrap();
        w.finish_on_last_piece();
        assert_eq!(w.volume_index(), 1);
        assert_eq!(w.offset_in_volume().unwrap(), 4);
        w.write_all(&[2u8; 22]).unwrap();
        assert_eq!(w.volume_index(), 1);
        assert_eq!(w.offset_in_volume().unwrap(), 26);
        assert_eq!(w.piece_count(), 2);
    }

    #[test]
    fn drop_deletes_partials() {
        let tmp = TempDir::new();
        let dest = tmp.path().join("out.zip");
        fs::write(&dest, b"KEEP").unwrap();
        {
            let mut w = SplitWriter::create(dest.clone(), 8).unwrap();
            w.write_all(&[7u8; 20]).unwrap();
        }
        assert_eq!(fs::read(&dest).unwrap(), b"KEEP");
        assert!(!dest.with_file_name("out.z01").exists());
        assert!(!partial_path_for(&dest, 1).exists());
        assert!(!partial_path_for(&dest, 2).exists());
        assert!(!partial_path_for(&dest, 3).exists());
    }
}

/// 単体テスト用。`tests/support` とは別（lib 内テストから TempDir を使う）。
#[cfg(test)]
mod tests_support {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new() -> Self {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "zipcore-split-{}-{}-{}",
                std::process::id(),
                nanos,
                n
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
