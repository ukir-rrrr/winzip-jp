//! PKZIP ローカル / 中央ディレクトリ / EOCD の書き込みと検査。
//!
//! ファイル名は呼び出し側が渡した CP932 バイトをそのまま書く。UTF-8 化しない。
//! extra field は非 AES なら空。AES なら 0x9901 のみ（0x7075 は書かない）。

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Datelike, Timelike};

use crate::aeswrite::extra_is_ae2_aes256;
use crate::encoding::{EFS_UTF8_BIT, UNICODE_PATH_EXTRA_ID};
use crate::error::ZipcoreError;

pub const SIG_LOCAL: u32 = 0x0403_4b50;
pub const SIG_CENTRAL: u32 = 0x0201_4b50;
pub const SIG_EOCD: u32 = 0x0605_4b50;

pub const METHOD_STORED: u16 = 0;
pub const METHOD_DEFLATE: u16 = 8;

/// APPNOTE 2.0（Deflate）。付録 A の `14 00`。
pub const VERSION_NEEDED: u16 = 20;
pub const HOST_OS_MSDOS: u8 = 0;
/// 下位バイト = 2.0、上位バイト = MS-DOS (0)。
pub const VERSION_MADE_BY: u16 = 20 | ((HOST_OS_MSDOS as u16) << 8);

/// 非暗号・非 UTF-8。bit 0 も bit 11 も立てない。
pub const GPBF_PLAIN: u16 = 0;
pub use crate::aeswrite::AES_EXTRA_ID;

pub const LOCAL_CRC_OFFSET: u64 = 14;
pub const DOS_ATTR_ARCHIVE: u32 = 0x20;
pub const DOS_ATTR_DIR: u32 = 0x10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectedEntry {
    pub name_raw: Vec<u8>,
    pub gpbf: u16,
    pub method: u16,
    pub host_os: u8,
    pub extra_local: Vec<u8>,
    pub extra_central: Vec<u8>,
    pub crc: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub version_needed: u16,
    pub version_made_by: u16,
}

impl InspectedEntry {
    pub fn has_unicode_path_extra(&self) -> bool {
        extra_has_id(&self.extra_local, UNICODE_PATH_EXTRA_ID)
            || extra_has_id(&self.extra_central, UNICODE_PATH_EXTRA_ID)
    }

    pub fn has_aes_extra(&self) -> bool {
        extra_is_ae2_aes256(&self.extra_local) && extra_is_ae2_aes256(&self.extra_central)
    }
}

#[derive(Clone, Debug)]
pub struct CentralRecord {
    pub name: Vec<u8>,
    pub extra: Vec<u8>,
    pub method: u16,
    pub gpbf: u16,
    pub crc: u32,
    pub compressed: u32,
    pub uncompressed: u32,
    pub local_offset: u32,
    pub dos_time: u16,
    pub dos_date: u16,
    pub version_needed: u16,
    pub is_dir: bool,
}

pub fn write_local_header<W: Write>(
    w: &mut W,
    name: &[u8],
    extra: &[u8],
    method: u16,
    gpbf: u16,
    dos_time: u16,
    dos_date: u16,
    crc: u32,
    compressed: u32,
    uncompressed: u32,
    version_needed: u16,
) -> io::Result<()> {
    debug_assert_eq!(gpbf & EFS_UTF8_BIT, 0, "bit 11 must never be set");
    debug_assert!(
        extra.is_empty() || extra_has_id(extra, AES_EXTRA_ID),
        "only AES extra 0x9901 may be written"
    );
    debug_assert!(
        !extra_has_id(extra, UNICODE_PATH_EXTRA_ID),
        "must not write 0x7075"
    );
    w.write_all(&SIG_LOCAL.to_le_bytes())?;
    w.write_all(&version_needed.to_le_bytes())?;
    w.write_all(&gpbf.to_le_bytes())?;
    w.write_all(&method.to_le_bytes())?;
    w.write_all(&dos_time.to_le_bytes())?;
    w.write_all(&dos_date.to_le_bytes())?;
    w.write_all(&crc.to_le_bytes())?;
    w.write_all(&compressed.to_le_bytes())?;
    w.write_all(&uncompressed.to_le_bytes())?;
    w.write_all(&(name.len() as u16).to_le_bytes())?;
    w.write_all(&(extra.len() as u16).to_le_bytes())?;
    w.write_all(name)?;
    w.write_all(extra)?;
    Ok(())
}

pub fn patch_local_sizes<W: Write + Seek>(
    w: &mut W,
    local_header_offset: u64,
    crc: u32,
    compressed: u32,
    uncompressed: u32,
) -> io::Result<()> {
    w.seek(SeekFrom::Start(local_header_offset + LOCAL_CRC_OFFSET))?;
    w.write_all(&crc.to_le_bytes())?;
    w.write_all(&compressed.to_le_bytes())?;
    w.write_all(&uncompressed.to_le_bytes())?;
    Ok(())
}

pub fn write_central_header<W: Write>(w: &mut W, rec: &CentralRecord) -> io::Result<()> {
    debug_assert_eq!(rec.gpbf & EFS_UTF8_BIT, 0, "bit 11 must never be set");
    let ext_attr = if rec.is_dir {
        DOS_ATTR_DIR
    } else {
        DOS_ATTR_ARCHIVE
    };
    w.write_all(&SIG_CENTRAL.to_le_bytes())?;
    debug_assert!(
        rec.extra.is_empty() || extra_has_id(&rec.extra, AES_EXTRA_ID),
        "only AES extra 0x9901 may be written"
    );
    debug_assert!(
        !extra_has_id(&rec.extra, UNICODE_PATH_EXTRA_ID),
        "must not write 0x7075"
    );
    w.write_all(&VERSION_MADE_BY.to_le_bytes())?;
    w.write_all(&rec.version_needed.to_le_bytes())?;
    w.write_all(&rec.gpbf.to_le_bytes())?;
    w.write_all(&rec.method.to_le_bytes())?;
    w.write_all(&rec.dos_time.to_le_bytes())?;
    w.write_all(&rec.dos_date.to_le_bytes())?;
    w.write_all(&rec.crc.to_le_bytes())?;
    w.write_all(&rec.compressed.to_le_bytes())?;
    w.write_all(&rec.uncompressed.to_le_bytes())?;
    w.write_all(&(rec.name.len() as u16).to_le_bytes())?;
    w.write_all(&(rec.extra.len() as u16).to_le_bytes())?;
    w.write_all(&0u16.to_le_bytes())?; // comment
    w.write_all(&0u16.to_le_bytes())?; // disk
    w.write_all(&0u16.to_le_bytes())?; // internal attr
    w.write_all(&ext_attr.to_le_bytes())?;
    w.write_all(&rec.local_offset.to_le_bytes())?;
    w.write_all(&rec.name)?;
    w.write_all(&rec.extra)?;
    Ok(())
}

pub fn write_eocd<W: Write>(
    w: &mut W,
    entry_count: u16,
    central_size: u32,
    central_offset: u32,
) -> io::Result<()> {
    w.write_all(&SIG_EOCD.to_le_bytes())?;
    w.write_all(&0u16.to_le_bytes())?; // this disk
    w.write_all(&0u16.to_le_bytes())?; // cd disk
    w.write_all(&entry_count.to_le_bytes())?;
    w.write_all(&entry_count.to_le_bytes())?;
    w.write_all(&central_size.to_le_bytes())?;
    w.write_all(&central_offset.to_le_bytes())?;
    w.write_all(&0u16.to_le_bytes())?; // comment
    Ok(())
}

pub fn msdos_datetime(ts: SystemTime) -> (u16, u16) {
    // APPNOTE の DOS 時刻はローカル時。UTC で詰めると Windows 側が時差ぶんずれる。
    if ts < UNIX_EPOCH {
        return (0, dos_date_1980_01_01());
    }
    let local = chrono::DateTime::<chrono::Local>::from(ts);
    pack_msdos(
        local.year(),
        local.month(),
        local.day(),
        local.hour(),
        local.minute(),
        local.second(),
    )
}

pub fn pack_msdos(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> (u16, u16) {
    if year < 1980 {
        return (0, dos_date_1980_01_01());
    }
    let year = (year - 1980).min(127) as u16;
    let date = (year << 9) | ((month as u16) << 5) | (day as u16);
    let time = ((hour as u16) << 11) | ((min as u16) << 5) | ((sec as u16) / 2);
    (time, date)
}

fn dos_date_1980_01_01() -> u16 {
    (1 << 5) | 1
}

pub fn inspect_zip(path: &Path) -> Result<Vec<InspectedEntry>, ZipcoreError> {
    let mut file = File::open(path)?;
    inspect_reader(&mut file)
}

pub fn inspect_reader<R: Read + Seek>(r: &mut R) -> Result<Vec<InspectedEntry>, ZipcoreError> {
    let eocd = find_eocd(r)?;
    r.seek(SeekFrom::Start(eocd))?;
    let mut hdr = [0u8; 22];
    r.read_exact(&mut hdr)?;
    if u32::from_le_bytes(hdr[0..4].try_into().unwrap()) != SIG_EOCD {
        return Err(ZipcoreError::Corrupt("EOCD 署名がありません".into()));
    }
    let cd_entries = u16::from_le_bytes(hdr[10..12].try_into().unwrap()) as usize;
    let cd_size = u32::from_le_bytes(hdr[12..16].try_into().unwrap()) as u64;
    let cd_offset = u32::from_le_bytes(hdr[16..20].try_into().unwrap()) as u64;

    r.seek(SeekFrom::Start(cd_offset))?;
    let mut cd = vec![0u8; cd_size as usize];
    r.read_exact(&mut cd)?;

    let mut entries = Vec::with_capacity(cd_entries);
    let mut i = 0usize;
    for _ in 0..cd_entries {
        if i + 46 > cd.len() {
            return Err(ZipcoreError::Corrupt(
                "中央ディレクトリが途中で切れています".into(),
            ));
        }
        let sig = u32::from_le_bytes(cd[i..i + 4].try_into().unwrap());
        if sig != SIG_CENTRAL {
            return Err(ZipcoreError::Corrupt(
                "中央ディレクトリ署名が違います".into(),
            ));
        }
        let version_made_by = u16::from_le_bytes(cd[i + 4..i + 6].try_into().unwrap());
        let host_os = (version_made_by >> 8) as u8;
        let version_needed = u16::from_le_bytes(cd[i + 6..i + 8].try_into().unwrap());
        let gpbf = u16::from_le_bytes(cd[i + 8..i + 10].try_into().unwrap());
        let method = u16::from_le_bytes(cd[i + 10..i + 12].try_into().unwrap());
        let crc = u32::from_le_bytes(cd[i + 16..i + 20].try_into().unwrap());
        let compressed = u32::from_le_bytes(cd[i + 20..i + 24].try_into().unwrap());
        let uncompressed = u32::from_le_bytes(cd[i + 24..i + 28].try_into().unwrap());
        let name_len = u16::from_le_bytes(cd[i + 28..i + 30].try_into().unwrap()) as usize;
        let extra_len = u16::from_le_bytes(cd[i + 30..i + 32].try_into().unwrap()) as usize;
        let comment_len = u16::from_le_bytes(cd[i + 32..i + 34].try_into().unwrap()) as usize;
        let local_offset = u32::from_le_bytes(cd[i + 42..i + 46].try_into().unwrap()) as u64;
        i += 46;
        if i + name_len + extra_len + comment_len > cd.len() {
            return Err(ZipcoreError::Corrupt(
                "中央ディレクトリの名前が切れています".into(),
            ));
        }
        let name_central = cd[i..i + name_len].to_vec();
        i += name_len;
        let extra_central = cd[i..i + extra_len].to_vec();
        i += extra_len + comment_len;

        r.seek(SeekFrom::Start(local_offset))?;
        let mut local = [0u8; 30];
        r.read_exact(&mut local)?;
        if u32::from_le_bytes(local[0..4].try_into().unwrap()) != SIG_LOCAL {
            return Err(ZipcoreError::Corrupt("ローカルヘッダ署名が違います".into()));
        }
        let local_gpbf = u16::from_le_bytes(local[6..8].try_into().unwrap());
        let local_method = u16::from_le_bytes(local[8..10].try_into().unwrap());
        let local_crc = u32::from_le_bytes(local[14..18].try_into().unwrap());
        let local_name_len = u16::from_le_bytes(local[26..28].try_into().unwrap()) as usize;
        let local_extra_len = u16::from_le_bytes(local[28..30].try_into().unwrap()) as usize;
        let mut name_local = vec![0u8; local_name_len];
        r.read_exact(&mut name_local)?;
        let mut extra_local = vec![0u8; local_extra_len];
        r.read_exact(&mut extra_local)?;

        if name_local != name_central {
            return Err(ZipcoreError::Corrupt(
                "ローカルと中央ディレクトリのファイル名バイトが一致しません".into(),
            ));
        }
        if local_gpbf != gpbf || local_method != method || local_crc != crc {
            return Err(ZipcoreError::Corrupt(
                "ローカルと中央ディレクトリのフラグ / メソッド / CRC が一致しません".into(),
            ));
        }

        entries.push(InspectedEntry {
            name_raw: name_central,
            gpbf,
            method,
            host_os,
            extra_local,
            extra_central,
            crc,
            compressed_size: compressed,
            uncompressed_size: uncompressed,
            version_needed,
            version_made_by,
        });
    }
    Ok(entries)
}

fn find_eocd<R: Read + Seek>(r: &mut R) -> Result<u64, ZipcoreError> {
    let len = r.seek(SeekFrom::End(0))?;
    if len < 22 {
        return Err(ZipcoreError::Corrupt("ファイルが短すぎます".into()));
    }
    let max_back = (len - 22).min(65_535);
    let start = len - 22 - max_back;
    r.seek(SeekFrom::Start(start))?;
    let mut buf = vec![0u8; (len - start) as usize];
    r.read_exact(&mut buf)?;
    let last = buf.len() - 22;
    for i in (0..=last).rev() {
        let sig = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap());
        if sig != SIG_EOCD {
            continue;
        }
        let comment_len = u16::from_le_bytes(buf[i + 20..i + 22].try_into().unwrap()) as usize;
        if i + 22 + comment_len == buf.len() {
            return Ok(start + i as u64);
        }
    }
    Err(ZipcoreError::Corrupt("EOCD が見つかりません".into()))
}

pub fn extra_has_id(extra: &[u8], id: u16) -> bool {
    let mut i = 0;
    while i + 4 <= extra.len() {
        let tag = u16::from_le_bytes([extra[i], extra[i + 1]]);
        let size = u16::from_le_bytes([extra[i + 2], extra[i + 3]]) as usize;
        i += 4;
        if i + size > extra.len() {
            return false;
        }
        if tag == id {
            return true;
        }
        i += size;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn gpbf_plain_never_sets_efs_or_encrypt() {
        assert_eq!(GPBF_PLAIN & EFS_UTF8_BIT, 0);
        assert_eq!(GPBF_PLAIN & 1, 0);
        assert_eq!(HOST_OS_MSDOS, 0);
        assert_eq!(VERSION_MADE_BY >> 8, HOST_OS_MSDOS as u16);
    }

    #[test]
    fn dos_time_matches_local_civil_time() {
        let ts = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let (time, date) = msdos_datetime(ts);
        let local = chrono::DateTime::<chrono::Local>::from(ts);
        let (want_time, want_date) = pack_msdos(
            local.year(),
            local.month(),
            local.day(),
            local.hour(),
            local.minute(),
            local.second(),
        );
        assert_eq!((time, date), (want_time, want_date));
    }

    /// TZ=UTC の CI でも UTC 実装への退行を検出する。POSIX `JST-9` は tzdata 不要。
    #[cfg(unix)]
    #[test]
    fn dos_time_jst_fixed_not_utc() {
        const INSTANT: u64 = 1_700_000_000; // 2023-11-14 22:13:20 UTC = 11-15 07:13:20 JST
        if std::env::var_os("ZIPCORE_DOS_TZ_CHILD").is_some() {
            let ts = UNIX_EPOCH + std::time::Duration::from_secs(INSTANT);
            let got = msdos_datetime(ts);
            assert_eq!(got, pack_msdos(2023, 11, 15, 7, 13, 20));
            assert_ne!(got, pack_msdos(2023, 11, 14, 22, 13, 20));
            return;
        }
        let exe = std::env::current_exe().expect("test exe");
        let status = std::process::Command::new(exe)
            .env("TZ", "JST-9")
            .env("ZIPCORE_DOS_TZ_CHILD", "1")
            .args(["pkzip::tests::dos_time_jst_fixed_not_utc", "--exact"])
            .status()
            .expect("spawn TZ child");
        assert!(
            status.success(),
            "JST 固定値テストの子プロセスが失敗: {status}"
        );
    }

    #[test]
    fn extra_has_id_detects_7075() {
        let mut extra = Vec::new();
        extra.extend_from_slice(&UNICODE_PATH_EXTRA_ID.to_le_bytes());
        extra.extend_from_slice(&1u16.to_le_bytes());
        extra.push(1);
        assert!(extra_has_id(&extra, UNICODE_PATH_EXTRA_ID));
        assert!(!extra_has_id(&[], UNICODE_PATH_EXTRA_ID));
    }
}
