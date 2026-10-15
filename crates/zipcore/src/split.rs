//! WinZip 形式の分割出力。ZIP の中身（エンコーディング / ヘッダ）は変えない。
//!
//! ピース名は `{stem}.z01` … 最後が `{stem}.zip`（`dest`）。
//! 非末尾ピースは `split_bytes` を超えない。キャンセル時は部分ピースを全部消す。

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::ZipcoreError;
use crate::fsutil::{atomic_replace, path_eq};
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
    if path_eq(dest_zip, candidate) {
        return true;
    }
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
    let bytes = join_volumes(zip_path)?;
    fs::write(dest, bytes)
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

/// 論理オフセットをピースへ割り振る `Write + Seek`。
pub struct SplitWriter {
    dest: PathBuf,
    split_bytes: u64,
    files: Vec<File>,
    paths: Vec<PathBuf>,
    pos: u64,
    len: u64,
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
        let mut placed = Vec::new();
        for (partial, final_path) in partials.iter().zip(finals.iter()) {
            match replace_file(partial, final_path) {
                Ok(()) => placed.push(final_path.clone()),
                Err(e) => {
                    discard_paths(&partials);
                    for p in &placed {
                        if !path_eq(p, &self.dest) {
                            let _ = fs::remove_file(p);
                        }
                    }
                    return Err(e.into());
                }
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

fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    match atomic_replace(from, to)? {
        Some(backup) => {
            let _ = fs::remove_file(&backup);
            Ok(())
        }
        None => Ok(()),
    }
}

impl Write for SplitWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
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
        if self.pos <= self.len && !self.files.is_empty() {
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

/// 連結ストリームが EOCD で終わり、その末尾が最後の `.zip` ピースに載っているか。
/// EOCD がピース境界をまたいでも、終端バイトが最後のピースにあればよい。
pub fn eocd_in_last_volume(zip_path: &Path) -> io::Result<bool> {
    let volumes = discover_volumes(zip_path);
    let last = volumes
        .last()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "分割ピースがありません"))?;
    let last_len = fs::metadata(last)?.len();
    if last_len == 0 {
        return Ok(false);
    }
    let joined = join_volumes(zip_path)?;
    if joined.len() < 22 {
        return Ok(false);
    }
    let sig = u32::from_le_bytes(
        joined[joined.len() - 22..joined.len() - 18]
            .try_into()
            .unwrap(),
    );
    Ok(sig == SIG_EOCD)
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
        assert!(is_split_sibling(dest, dest));
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
    fn eocd_may_straddle_piece_boundary() {
        let tmp = TempDir::new();
        let dest = tmp.path().join("out.zip");
        let mut w = SplitWriter::create(dest.clone(), 16).unwrap();
        w.write_all(&[1u8; 20]).unwrap();
        let mut eocd = [0u8; 22];
        eocd[0..4].copy_from_slice(&SIG_EOCD.to_le_bytes());
        w.write_all(&eocd).unwrap();
        w.finish().unwrap();
        assert!(eocd_in_last_volume(&dest).unwrap());
        assert!(fs::metadata(&dest).unwrap().len() < 22);
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
