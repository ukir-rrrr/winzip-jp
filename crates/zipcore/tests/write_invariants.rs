//! 設計 §2.3 の作成不変条件を temp ツリーで固定する。

mod support;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use unicode_normalization::UnicodeNormalization;
use zipcore::{
    encode_entry_name, inspect_zip_headers, plan_write, write_zip, AesMode, InputItem,
    ProgressPhase, SecretString, WriteRequest, ZipcoreError, EFS_UTF8_BIT, UNICODE_PATH_EXTRA_ID,
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

fn zip_inputs(root: &Path, dest: &Path, replacements: BTreeMap<String, String>) -> PathBuf {
    let mut inputs = Vec::new();
    for ent in fs::read_dir(root).unwrap() {
        let ent = ent.unwrap();
        let ft = ent.file_type().unwrap();
        inputs.push(InputItem {
            abs_path: ent.path(),
            as_dir: ft.is_dir(),
        });
    }
    let req = WriteRequest {
        inputs,
        dest: dest.to_path_buf(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements,
    };
    let cancel = AtomicBool::new(false);
    write_zip(req, |_| {}, &cancel).expect("write_zip");
    dest.to_path_buf()
}

fn assert_invariants(zip_path: &Path, src_names: &[(&str, bool)]) {
    let entries = inspect_zip_headers(zip_path).unwrap();
    assert!(!entries.is_empty());
    for e in &entries {
        assert_eq!(
            e.gpbf & EFS_UTF8_BIT,
            0,
            "bit 11 must be 0: {}",
            hex(&e.name_raw)
        );
        assert_eq!(e.gpbf & 1, 0, "bit 0 must be 0");
        assert_eq!(e.host_os, 0, "host OS must be MS-DOS");
        assert_eq!(e.version_made_by >> 8, 0);
        assert!(!e.has_unicode_path_extra(), "must not write 0x7075");
        assert!(!zipcore::write::InspectedEntry::has_unicode_path_extra(e));
        assert!(!extra_has(e.extra_local.as_slice(), UNICODE_PATH_EXTRA_ID));
        assert!(!extra_has(
            e.extra_central.as_slice(),
            UNICODE_PATH_EXTRA_ID
        ));
        assert!(e.extra_local.is_empty());
        assert!(e.extra_central.is_empty());
        if e.method == 0 {
            assert!(
                e.name_raw.ends_with(b"/"),
                "Stored entry must be a directory with trailing slash"
            );
            assert_eq!(e.compressed_size, 0);
            assert_eq!(e.uncompressed_size, 0);
        } else {
            assert_eq!(e.method, 8, "files must be Deflate");
        }
    }
    for (name, is_dir) in src_names {
        let dir_name = format!("{name}/");
        let encoded = encode_entry_name(if *is_dir { dir_name.as_str() } else { *name });
        let found = entries.iter().find(|e| e.name_raw == encoded.cp932);
        assert!(
            found.is_some(),
            "missing {} hex {}",
            name,
            hex(&encoded.cp932)
        );
        assert_eq!(found.unwrap().name_raw, encoded.cp932);
    }
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

#[test]
fn japanese_and_ascii_names_are_cp932_with_bit11_down() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("が.txt"), b"ga");
    write_file(&src.join("hello.txt"), b"hi");
    write_file(
        &src.join("書類").join("こんにちは.txt"),
        "こんにちは".as_bytes(),
    );
    let zip = tmp.path().join("out.zip");
    zip_inputs(&src, &zip, BTreeMap::new());
    assert_invariants(
        &zip,
        &[
            ("が.txt", false),
            ("hello.txt", false),
            ("書類", true),
            ("書類/こんにちは.txt", false),
        ],
    );
    let entries = inspect_zip_headers(&zip).unwrap();
    let ga = entries
        .iter()
        .find(|e| e.name_raw == encode_entry_name("が.txt").cp932)
        .unwrap();
    assert_eq!(hex(&ga.name_raw), "82AA2E747874");
    assert_ne!(ga.name_raw, "が.txt".as_bytes());
    assert!(std::str::from_utf8(&ga.name_raw).is_err());
    let hello = entries.iter().find(|e| e.name_raw == b"hello.txt").unwrap();
    assert!(std::str::from_utf8(&hello.name_raw).is_ok());
    assert_eq!(hello.gpbf & EFS_UTF8_BIT, 0);
}

#[test]
fn nfd_on_disk_is_nfc_cp932_in_zip() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    let nfd: String = "が.txt".nfd().collect();
    write_file(&src.join(&nfd), b"nfd");
    let zip = tmp.path().join("out.zip");
    zip_inputs(&src, &zip, BTreeMap::new());
    let entries = inspect_zip_headers(&zip).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(hex(&entries[0].name_raw), "82AA2E747874");
    assert_eq!(entries[0].name_raw, encode_entry_name(&nfd).cp932);
    assert_eq!(entries[0].gpbf & EFS_UTF8_BIT, 0);
}

#[test]
fn junk_is_absent_from_central_directory() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("keep.txt"), b"k");
    write_file(&src.join(".DS_Store"), b"ds");
    write_file(&src.join("__MACOSX").join("._x"), b"x");
    write_file(&src.join("nested").join(".DS_Store"), b"ds");
    write_file(&src.join("nested").join("ok.txt"), b"ok");
    let zip = tmp.path().join("out.zip");
    let dest = zip_inputs(&src, &zip, BTreeMap::new());
    let entries = inspect_zip_headers(&dest).unwrap();
    let names: Vec<Vec<u8>> = entries.iter().map(|e| e.name_raw.clone()).collect();
    assert!(names.contains(&b"keep.txt".to_vec()));
    assert!(names.contains(&b"nested/".to_vec()));
    assert!(names.contains(&b"nested/ok.txt".to_vec()));
    for n in &names {
        let s = String::from_utf8_lossy(n);
        assert!(!s.contains(".DS_Store"), "{s}");
        assert!(!s.contains("__MACOSX"), "{s}");
    }
}

#[test]
fn junk_only_inputs_error() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join(".DS_Store"), b"ds");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src.join(".DS_Store"),
            as_dir: false,
        }],
        dest: tmp.path().join("out.zip"),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    let err = write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(err, ZipcoreError::EmptyInputs));
}

#[test]
fn unconfirmed_replacements_refuse_to_write() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("髙.txt"), b"x");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src.join("髙.txt"),
            as_dir: false,
        }],
        dest: tmp.path().join("out.zip"),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    let plan = plan_write(&req).unwrap();
    assert!(!plan.issues.is_empty());
    let err = write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(err, ZipcoreError::UnconfirmedReplacements { .. }));
    assert!(!tmp.path().join("out.zip").exists());
}

#[test]
fn confirmed_replacement_and_collision_suffix() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("髙.txt"), b"taka");
    write_file(&src.join("高.txt"), b"taka2");
    let mut replacements = BTreeMap::new();
    replacements.insert("髙.txt".into(), "高.txt".into());
    let zip = tmp.path().join("out.zip");
    zip_inputs(&src, &zip, replacements);
    let entries = inspect_zip_headers(&zip).unwrap();
    let names: Vec<String> = entries.iter().map(|e| hex(&e.name_raw)).collect();
    assert!(names.contains(&hex(&encode_entry_name("高.txt").cp932)));
    assert!(names.contains(&hex(&encode_entry_name("高 (2).txt").cp932)));
    assert_eq!(entries.len(), 2);
    for e in &entries {
        assert_eq!(e.gpbf & EFS_UTF8_BIT, 0);
        assert_eq!(e.method, 8);
    }
}

#[test]
fn password_with_only_empty_dirs_is_rejected() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    fs::create_dir_all(src.join("空")).unwrap();
    let dest = tmp.path().join("out.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: true,
        }],
        dest: dest.clone(),
        password: Some(SecretString::new("secret")),
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    let err = write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(err, ZipcoreError::NoEncryptableFiles));
    assert_eq!(err.to_string(), "パスワード付きにするファイルがありません");
    assert!(!dest.exists());
}

#[test]
fn empty_directory_is_stored_with_trailing_slash() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    fs::create_dir_all(src.join("空").join("中")).unwrap();
    let zip = tmp.path().join("out.zip");
    zip_inputs(&src, &zip, BTreeMap::new());
    let entries = inspect_zip_headers(&zip).unwrap();
    let empty = entries
        .iter()
        .find(|e| e.name_raw == encode_entry_name("空/").cp932)
        .unwrap();
    assert_eq!(empty.method, 0);
    assert!(empty.name_raw.ends_with(b"/"));
    let nested = entries
        .iter()
        .find(|e| e.name_raw == encode_entry_name("空/中/").cp932)
        .unwrap();
    assert_eq!(nested.method, 0);
}

#[test]
fn crc_and_size_match_source_file() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    let body = b"crc-check-payload-0123456789";
    write_file(&src.join("a.txt"), body);
    let zip = tmp.path().join("out.zip");
    zip_inputs(&src, &zip, BTreeMap::new());
    let entries = inspect_zip_headers(&zip).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].crc, crc32fast::hash(body));
    assert_eq!(entries[0].uncompressed_size, body.len() as u32);
    assert!(entries[0].compressed_size > 0);
}

#[test]
fn cancel_before_write_leaves_no_dest() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("a.txt"), b"a");
    let dest = tmp.path().join("out.zip");
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
    let cancel = AtomicBool::new(false);
    let err = write_zip(
        req,
        |p| {
            if p.phase == ProgressPhase::Starting {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    )
    .unwrap_err();
    assert!(matches!(err, ZipcoreError::Cancelled));
    assert!(!dest.exists());
}

#[test]
fn first_progress_is_starting() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("a.txt"), b"a");
    let phases = Mutex::new(Vec::new());
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src.join("a.txt"),
            as_dir: false,
        }],
        dest: tmp.path().join("out.zip"),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    write_zip(
        req,
        |p| phases.lock().unwrap().push(p.phase),
        &AtomicBool::new(false),
    )
    .unwrap();
    let phases = phases.lock().unwrap();
    assert_eq!(phases.first().copied(), Some(ProgressPhase::Starting));
    assert_eq!(phases.last().copied(), Some(ProgressPhase::Done));
}

#[test]
fn empty_password_is_rejected() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("a.txt");
    write_file(&src, b"a");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: false,
        }],
        dest: tmp.path().join("out.zip"),
        password: Some(SecretString::new("")),
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    let err = write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(err, ZipcoreError::EmptyPassword));
    assert_eq!(err.to_string(), "パスワードを入力してください");
}

#[test]
fn aes_zip_sets_encrypt_bit_and_9901_and_keeps_cp932_names() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("が.txt"), b"ga");
    write_file(&src.join("hello.txt"), b"hi");
    let dest = tmp.path().join("aes.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: true,
        }],
        dest: dest.clone(),
        password: Some(SecretString::new("secret")),
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).expect("aes write");
    let entries = inspect_zip_headers(&dest).unwrap();
    let files: Vec<_> = entries
        .iter()
        .filter(|e| !e.name_raw.ends_with(b"/"))
        .collect();
    assert!(!files.is_empty());
    for e in &files {
        assert_eq!(e.gpbf & EFS_UTF8_BIT, 0);
        assert_eq!(e.gpbf & 1, 1);
        assert_eq!(e.method, 99);
        assert!(e.has_aes_extra());
        assert!(!e.has_unicode_path_extra());
        assert_eq!(e.host_os, 0);
        assert_eq!(e.crc, 0);
    }
    let ga = entries
        .iter()
        .find(|e| e.name_raw == encode_entry_name("src/が.txt").cp932)
        .unwrap();
    assert_eq!(ga.name_raw, encode_entry_name("src/が.txt").cp932);
}

#[test]
fn passwordless_zip_matches_aes_zip_names_and_bit11() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("漢字.txt"), b"kanji");
    let plain = tmp.path().join("plain.zip");
    let aes = tmp.path().join("aes.zip");
    let base = || WriteRequest {
        inputs: vec![InputItem {
            abs_path: src.clone(),
            as_dir: true,
        }],
        dest: PathBuf::new(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    let mut plain_req = base();
    plain_req.dest = plain.clone();
    write_zip(plain_req, |_| {}, &AtomicBool::new(false)).unwrap();
    let mut aes_req = base();
    aes_req.dest = aes.clone();
    aes_req.password = Some(SecretString::new("secret"));
    write_zip(aes_req, |_| {}, &AtomicBool::new(false)).unwrap();

    let plain_entries = inspect_zip_headers(&plain).unwrap();
    let aes_entries = inspect_zip_headers(&aes).unwrap();
    let plain_names: Vec<_> = plain_entries.iter().map(|e| e.name_raw.clone()).collect();
    let aes_names: Vec<_> = aes_entries.iter().map(|e| e.name_raw.clone()).collect();
    assert_eq!(plain_names, aes_names);
    for e in &plain_entries {
        assert_eq!(e.gpbf & EFS_UTF8_BIT, 0);
        assert_eq!(e.gpbf & 1, 0);
    }
    for e in aes_entries.iter().filter(|e| !e.name_raw.ends_with(b"/")) {
        assert_eq!(e.gpbf & EFS_UTF8_BIT, 0);
        assert_eq!(e.gpbf & 1, 1);
    }
}

#[test]
fn japanese_password_is_utf8_bytes() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("a.txt");
    write_file(&src, b"payload");
    let dest = tmp.path().join("jp.zip");
    let password = SecretString::new("パスワード");
    assert_eq!(password.expose_bytes(), "パスワード".as_bytes());
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: false,
        }],
        dest: dest.clone(),
        password: Some(password),
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap();
    let dest_out = tmp.path().join("out");
    fs::create_dir_all(&dest_out).unwrap();
    let missing = zipcore::extract_zip(
        zipcore::ExtractRequest {
            archive: dest.clone(),
            dest: dest_out.clone(),
            encoding: None,
            overwrite: false,
            password: None,
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(missing, ZipcoreError::Encrypted { .. }));
    zipcore::extract_zip(
        zipcore::ExtractRequest {
            archive: dest,
            dest: dest_out.clone(),
            encoding: None,
            overwrite: false,
            password: Some(SecretString::new("パスワード")),
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .expect("utf-8 password decrypt");
    assert_eq!(fs::read(dest_out.join("a.txt")).unwrap(), b"payload");
}

#[test]
fn aes_extract_without_password_fails_before_writing() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("書類").join("a.txt"), b"plain");
    write_file(&src.join("b.txt"), b"other");
    let dest = tmp.path().join("aes.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: true,
        }],
        dest: dest.clone(),
        password: Some(SecretString::new("secret")),
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap();
    let dest_out = tmp.path().join("out");
    fs::create_dir_all(&dest_out).unwrap();
    let err = zipcore::extract_zip(
        zipcore::ExtractRequest {
            archive: dest,
            dest: dest_out.clone(),
            encoding: None,
            overwrite: false,
            password: None,
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(err, ZipcoreError::Encrypted { .. }));
    assert!(!dest_out.join("src").exists());
    assert!(!dest_out.join("src").join("b.txt").exists());
}

#[test]
fn aes_tampered_ciphertext_is_rejected() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("a.txt");
    write_file(&src, b"payload-for-hmac");
    let dest = tmp.path().join("aes.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: false,
        }],
        dest: dest.clone(),
        password: Some(SecretString::new("secret")),
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap();
    let entries = inspect_zip_headers(&dest).unwrap();
    let file = entries
        .iter()
        .find(|e| !e.name_raw.ends_with(b"/"))
        .unwrap();
    let data_start = 30 + file.name_raw.len() + file.extra_local.len();
    let flip = data_start + 18 + 4;
    let mut bytes = fs::read(&dest).unwrap();
    assert!(flip < bytes.len(), "ciphertext offset {flip}");
    bytes[flip] ^= 0x01;
    fs::write(&dest, bytes).unwrap();
    let dest_out = tmp.path().join("out");
    fs::create_dir_all(&dest_out).unwrap();
    let err = zipcore::extract_zip(
        zipcore::ExtractRequest {
            archive: dest,
            dest: dest_out.clone(),
            encoding: None,
            overwrite: false,
            password: Some(SecretString::new("secret")),
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(err, ZipcoreError::Io(_)),
        "tamper must fail HMAC/auth, got {err:?}"
    );
    assert_eq!(err.to_string(), "入出力エラー: データが不正です");
    assert!(!dest_out.join("a.txt").exists());
}

#[test]
fn dest_inside_input_dir_is_rejected() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("a.txt"), b"a");
    let dest = src.join("out.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: true,
        }],
        dest: dest.clone(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    let err = write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(err, ZipcoreError::DestOverlapsInput { .. }));
    assert!(!dest.exists());
}

#[test]
fn dest_with_dotdot_inside_input_is_rejected() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("a.txt"), b"a");
    let dest = tmp
        .path()
        .join("other")
        .join("..")
        .join("src")
        .join("out.zip");
    let req = WriteRequest {
        inputs: vec![InputItem {
            abs_path: src,
            as_dir: true,
        }],
        dest,
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    };
    let err = write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(err, ZipcoreError::DestOverlapsInput { .. }));
}

#[test]
fn cancel_does_not_delete_existing_dest() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    write_file(&src.join("a.txt"), b"payload-that-is-not-tiny-xxxxxxxx");
    let dest = tmp.path().join("keep.zip");
    fs::write(&dest, b"EXISTING").unwrap();
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
    let partial = {
        let mut n = dest.as_os_str().to_os_string();
        n.push(".partial");
        PathBuf::from(n)
    };
    assert!(!partial.exists());
}

#[test]
fn path_dotdot_from_encode_never_lands_in_zip_name() {
    let encoded = encode_entry_name("foo/../bar.txt");
    assert_eq!(encoded.cp932, b"foo/_/bar.txt");
    assert!(!encoded.cp932.windows(2).any(|w| w == b".."));
}

#[test]
fn secret_string_debug_redacts() {
    let s = SecretString::new("hunter2");
    assert_eq!(s.expose(), "hunter2");
    assert!(!format!("{s:?}").contains("hunter2"));
}
