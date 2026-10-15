//! 分割 ZIP の作成不変条件。非分割の不変条件は `write_invariants.rs`。

mod support;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use zipcore::{
    encode_entry_name, inspect_zip_headers, write_zip, AesMode, InputItem, ProgressPhase,
    SecretString, WriteRequest, ZipcoreError, EFS_UTF8_BIT, UNICODE_PATH_EXTRA_ID,
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn write_file(path: &Path, body: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = File::create(path).unwrap();
    f.write_all(body).unwrap();
}

fn extra_has(extra: &[u8], id: u16) -> bool {
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

fn assert_plain_invariants(zip_path: &Path, src_names: &[(&str, bool)]) {
    let entries = inspect_zip_headers(zip_path).unwrap();
    assert!(!entries.is_empty());
    for e in &entries {
        assert_eq!(e.gpbf & EFS_UTF8_BIT, 0);
        assert_eq!(e.gpbf & 1, 0);
        assert_eq!(e.host_os, 0);
        assert!(!e.has_unicode_path_extra());
        assert!(!extra_has(e.extra_local.as_slice(), UNICODE_PATH_EXTRA_ID));
        assert!(e.extra_local.is_empty());
        assert!(e.extra_central.is_empty());
    }
    for (name, is_dir) in src_names {
        let dir_name = format!("{name}/");
        let encoded = encode_entry_name(if *is_dir { dir_name.as_str() } else { *name });
        assert!(
            entries.iter().any(|e| e.name_raw == encoded.cp932),
            "missing {} hex {}",
            name,
            hex(&encoded.cp932)
        );
    }
}

fn assert_local_headers_fit_their_volume(zip_path: &Path) {
    let volumes = zipcore::discover_volumes(zip_path);
    let sizes: Vec<u64> = volumes
        .iter()
        .map(|p| fs::metadata(p).unwrap().len())
        .collect();
    let last = fs::read(volumes.last().unwrap()).unwrap();
    let (_this_disk, _cd_disk, cd_size, cd_offset) =
        zipcore::split::last_volume_eocd(zip_path).unwrap();
    let cd = &last[cd_offset as usize..cd_offset as usize + cd_size as usize];
    let mut i = 0usize;
    while i + 46 <= cd.len() {
        assert_eq!(&cd[i..i + 4], b"PK\x01\x02");
        let name_len = u16::from_le_bytes(cd[i + 28..i + 30].try_into().unwrap()) as usize;
        let extra_len = u16::from_le_bytes(cd[i + 30..i + 32].try_into().unwrap()) as usize;
        let comment_len = u16::from_le_bytes(cd[i + 32..i + 34].try_into().unwrap()) as usize;
        let disk = u16::from_le_bytes(cd[i + 34..i + 36].try_into().unwrap()) as usize;
        let local = u32::from_le_bytes(cd[i + 42..i + 46].try_into().unwrap()) as u64;
        let header_len = 30u64 + name_len as u64 + extra_len as u64;
        assert!(disk < sizes.len(), "disk {disk} >= {}", sizes.len());
        assert!(
            local + header_len <= sizes[disk],
            "local header straddles volume {disk}: off={local} hdr={header_len} size={}",
            sizes[disk]
        );
        let piece = fs::read(&volumes[disk]).unwrap();
        let off = local as usize;
        let sig = u32::from_le_bytes(piece[off..off + 4].try_into().unwrap());
        assert_eq!(sig, 0x0403_4b50, "local header missing on volume {disk}");
        i += 46 + name_len + extra_len + comment_len;
    }
}

fn incompressible(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i.wrapping_mul(167) + 13) as u8).collect()
}

#[test]
fn zero_split_bytes_is_rejected() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("a.txt");
    write_file(&src, b"a");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: false,
        }],
        dest: tmp.path().join("out.zip"),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: Some(0),
        replacements: BTreeMap::new(),
    };
    let err = write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(err, ZipcoreError::InvalidSplit { .. }));
}

#[test]
fn split_bytes_smaller_than_local_header_is_rejected() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("が.txt"), b"x");
    let dest = tmp.path().join("out.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src.join("が.txt"),
            as_dir: false,
        }],
        dest,
        password: None,
        aes: AesMode::Aes256,
        split_bytes: Some(20),
        replacements: BTreeMap::new(),
    };
    let err = write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_err();
    match err {
        ZipcoreError::InvalidSplit { reason } => {
            assert!(reason.contains("ヘッダ"), "{reason}");
        }
        other => panic!("expected InvalidSplit, got {other}"),
    }
}

#[test]
fn split_pieces_stay_within_limit_and_cd_ends_on_zip() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("が.txt"), &incompressible(2400));
    write_file(&src.join("blob.bin"), &incompressible(1800));
    let dest = tmp.path().join("out.zip");
    let split_bytes = 512u64;
    let req = WriteRequest {
        inputs: vec![
            InputItem {
                abs_path: src.join("が.txt"),
                as_dir: false,
            },
            InputItem {
                abs_path: src.join("blob.bin"),
                as_dir: false,
            },
        ],
        dest: dest.clone(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: Some(split_bytes),
        replacements: BTreeMap::new(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).expect("split write");
    let volumes = zipcore::discover_volumes(&dest);
    assert!(
        volumes.len() >= 2,
        "expected multiple pieces, got {}",
        volumes.len()
    );
    assert!(volumes[0].ends_with("out.z01"));
    assert_eq!(volumes.last().unwrap(), &dest);
    let sizes = zipcore::split::piece_sizes(&dest).unwrap();
    for (i, size) in sizes.iter().enumerate() {
        if i + 1 < sizes.len() {
            assert!(*size <= split_bytes, "piece {i} is {size} > {split_bytes}");
        }
    }
    assert!(zipcore::split::eocd_in_last_volume(&dest).unwrap());
    let last_len = fs::metadata(&dest).unwrap().len();
    let (this_disk, cd_disk, cd_size, cd_offset) = zipcore::split::last_volume_eocd(&dest).unwrap();
    assert_eq!(this_disk as usize, volumes.len() - 1);
    assert_eq!(cd_disk, this_disk);
    assert!(this_disk >= 1, "7-Zip skips .z01 when ThisDisk is 0");
    assert_eq!(u64::from(cd_offset) + u64::from(cd_size) + 22, last_len);
    assert_local_headers_fit_their_volume(&dest);
    assert_plain_invariants(&dest, &[("が.txt", false), ("blob.bin", false)]);
    let entries = inspect_zip_headers(&dest).unwrap();
    let ga = entries
        .iter()
        .find(|e| e.name_raw == encode_entry_name("が.txt").cp932)
        .unwrap();
    assert_eq!(hex(&ga.name_raw), "82AA2E747874");
    assert_eq!(ga.gpbf & EFS_UTF8_BIT, 0);
}

#[test]
fn split_smaller_than_limit_is_single_zip() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("a.txt"), b"tiny");
    let dest = tmp.path().join("out.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src.join("a.txt"),
            as_dir: false,
        }],
        dest: dest.clone(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: Some(1024 * 1024),
        replacements: BTreeMap::new(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap();
    assert!(dest.exists());
    assert!(!dest.with_file_name("out.z01").exists());
    assert_plain_invariants(&dest, &[("a.txt", false)]);
}

#[test]
fn non_split_save_does_not_delete_neighbor_z01() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("a.txt"), b"a");
    let dest = tmp.path().join("out.zip");
    let z01 = dest.with_file_name("out.z01");
    fs::write(&z01, b"keep me").unwrap();
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src.join("a.txt"),
            as_dir: false,
        }],
        dest: dest.clone(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).expect("single");
    assert_eq!(fs::read(&z01).unwrap(), b"keep me");
    assert!(dest.exists());
}

#[test]
fn leftover_split_pieces_do_not_spoil_later_single_zip() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("が.txt"), &incompressible(2400));
    write_file(&src.join("blob.bin"), &incompressible(1800));
    let dest = tmp.path().join("out.zip");
    let req = WriteRequest {
        inputs: vec![
            InputItem {
                abs_path: src.join("が.txt"),
                as_dir: false,
            },
            InputItem {
                abs_path: src.join("blob.bin"),
                as_dir: false,
            },
        ],
        dest: dest.clone(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: Some(512),
        replacements: BTreeMap::new(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).expect("split");
    let z01 = dest.with_file_name("out.z01");
    assert!(z01.exists());
    let src2 = tmp.path().join("src2");
    write_file(&src2.join("が.txt"), b"one");
    let req2 = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src2.join("が.txt"),
            as_dir: false,
        }],
        dest: dest.clone(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    write_zip(req2, |_| {}, &AtomicBool::new(false)).expect("single");
    assert!(z01.exists(), "non-split must not delete neighbor z01");
    fs::write(&z01, b"stale leftover").unwrap();
    let headers = inspect_zip_headers(&dest).unwrap();
    let file = headers
        .iter()
        .find(|e| e.name_raw.ends_with(b".txt"))
        .unwrap();
    assert_eq!(hex(&file.name_raw), "82AA2E747874");
    assert_eq!(file.gpbf & EFS_UTF8_BIT, 0);
    assert_eq!(
        headers
            .iter()
            .filter(|e| !e.name_raw.ends_with(b"/"))
            .count(),
        1
    );
}

#[test]
fn dest_split_sibling_input_is_rejected_even_if_case_differs() {
    let tmp = support::TempDir::new();
    let z01 = tmp.path().join("out.z01");
    write_file(&z01, b"old piece");
    let dest = tmp.path().join("OUT.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: z01.clone(),
            as_dir: false,
        }],
        dest: dest.clone(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    let err = write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(err, ZipcoreError::DestOverlapsInput { .. }));
    assert_eq!(fs::read(&z01).unwrap(), b"old piece");
    assert!(!dest.exists());
}

#[test]
fn split_cancel_deletes_every_piece() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("a.bin"), &incompressible(80 * 1024));
    write_file(&src.join("b.bin"), &incompressible(80 * 1024));
    let dest = tmp.path().join("out.zip");
    fs::write(&dest, b"EXISTING").unwrap();
    let z01 = dest.with_file_name("out.z01");
    fs::write(&z01, b"OLDZ01").unwrap();
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: true,
        }],
        dest: dest.clone(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: Some(1024),
        replacements: BTreeMap::new(),
    };
    let cancel = AtomicBool::new(false);
    let err = write_zip(
        req,
        |p| {
            if p.phase == ProgressPhase::Writing {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    )
    .unwrap_err();
    assert!(matches!(err, ZipcoreError::Cancelled));
    assert_eq!(fs::read(&dest).unwrap(), b"EXISTING");
    assert_eq!(fs::read(&z01).unwrap(), b"OLDZ01");
    for i in 1..=20u32 {
        let mut name = dest.file_stem().unwrap().to_os_string();
        name.push(format!(".z{i:02}.partial"));
        let partial = dest.parent().unwrap().join(name);
        assert!(!partial.exists(), "leftover {}", partial.display());
    }
}

#[test]
fn split_with_password_keeps_cp932_and_encrypt_bit() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("が.txt"), &incompressible(12 * 1024));
    let dest = tmp.path().join("out.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src.join("が.txt"),
            as_dir: false,
        }],
        dest: dest.clone(),
        password: Some(SecretString::new("secret")),
        aes: AesMode::Aes256,
        split_bytes: Some(256),
        replacements: BTreeMap::new(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).expect("split+aes");
    let volumes = zipcore::discover_volumes(&dest);
    assert!(volumes.len() >= 2);
    assert_local_headers_fit_their_volume(&dest);
    let entries = inspect_zip_headers(&dest).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name_raw, encode_entry_name("が.txt").cp932);
    assert_eq!(entries[0].gpbf & EFS_UTF8_BIT, 0);
    assert_eq!(entries[0].gpbf & 1, 1);
    assert!(entries[0].has_aes_extra());
    let joined = tmp.path().join("joined.zip");
    zipcore::join_volumes_to(&dest, &joined).unwrap();
    zipcore::extract_zip(
        zipcore::ExtractRequest {
            archive: joined,
            dest: tmp.path().join("out"),
            encoding: None,
            overwrite: false,
            password: Some(SecretString::new("secret")),
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .expect("decrypt split aes");
    assert_eq!(
        fs::read(tmp.path().join("out").join("が.txt")).unwrap(),
        incompressible(12 * 1024)
    );
}
