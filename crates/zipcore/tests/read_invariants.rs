//! 設計 §2.3 / §3.4 / §4.4 の解凍不変条件。

mod support;

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use zipcore::{
    extract_zip, list_zip, write_zip, AesMode, ExtractRequest, InputItem, ListRequest,
    NameEncoding, ProgressPhase, SecretString, WriteRequest, ZipcoreError, EFS_UTF8_BIT,
};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn fixture_dir() -> PathBuf {
    workspace_root().join("tests/fixtures/read")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn write_stored_zip(path: &Path, entries: &[(&[u8], &[u8], u16)]) {
    write_stored_zip_ex(path, entries, 0, 0x20)
}

fn write_stored_zip_ex(path: &Path, entries: &[(&[u8], &[u8], u16)], host_os: u8, ext_attr: u32) {
    let mut locals = Vec::new();
    let mut centrals = Vec::new();
    for (name, data, gpbf) in entries {
        let crc = crc32fast::hash(data);
        let size = data.len() as u32;
        let local_offset = locals.len() as u32;
        locals.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        locals.extend_from_slice(&20u16.to_le_bytes());
        locals.extend_from_slice(&gpbf.to_le_bytes());
        locals.extend_from_slice(&0u16.to_le_bytes());
        locals.extend_from_slice(&0u16.to_le_bytes());
        locals.extend_from_slice(&((1u16 << 5) | 1).to_le_bytes());
        locals.extend_from_slice(&crc.to_le_bytes());
        locals.extend_from_slice(&size.to_le_bytes());
        locals.extend_from_slice(&size.to_le_bytes());
        locals.extend_from_slice(&(name.len() as u16).to_le_bytes());
        locals.extend_from_slice(&0u16.to_le_bytes());
        locals.extend_from_slice(name);
        locals.extend_from_slice(data);

        let made_by = 20u16 | ((host_os as u16) << 8);
        centrals.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        centrals.extend_from_slice(&made_by.to_le_bytes());
        centrals.extend_from_slice(&20u16.to_le_bytes());
        centrals.extend_from_slice(&gpbf.to_le_bytes());
        centrals.extend_from_slice(&0u16.to_le_bytes());
        centrals.extend_from_slice(&0u16.to_le_bytes());
        centrals.extend_from_slice(&((1u16 << 5) | 1).to_le_bytes());
        centrals.extend_from_slice(&crc.to_le_bytes());
        centrals.extend_from_slice(&size.to_le_bytes());
        centrals.extend_from_slice(&size.to_le_bytes());
        centrals.extend_from_slice(&(name.len() as u16).to_le_bytes());
        centrals.extend_from_slice(&0u16.to_le_bytes());
        centrals.extend_from_slice(&0u16.to_le_bytes());
        centrals.extend_from_slice(&0u16.to_le_bytes());
        centrals.extend_from_slice(&0u16.to_le_bytes());
        centrals.extend_from_slice(&ext_attr.to_le_bytes());
        centrals.extend_from_slice(&local_offset.to_le_bytes());
        centrals.extend_from_slice(name);
    }
    let cd_offset = locals.len() as u32;
    let cd_size = centrals.len() as u32;
    let count = entries.len() as u16;
    let mut out = File::create(path).unwrap();
    out.write_all(&locals).unwrap();
    out.write_all(&centrals).unwrap();
    out.write_all(&0x0605_4b50u32.to_le_bytes()).unwrap();
    out.write_all(&0u16.to_le_bytes()).unwrap();
    out.write_all(&0u16.to_le_bytes()).unwrap();
    out.write_all(&count.to_le_bytes()).unwrap();
    out.write_all(&count.to_le_bytes()).unwrap();
    out.write_all(&cd_size.to_le_bytes()).unwrap();
    out.write_all(&cd_offset.to_le_bytes()).unwrap();
    out.write_all(&0u16.to_le_bytes()).unwrap();
}

fn extract_req(archive: PathBuf, dest: PathBuf, overwrite: bool) -> ExtractRequest {
    ExtractRequest {
        archive,
        dest,
        encoding: None,
        overwrite,
        password: None,
    }
}

fn list_auto(archive: &Path) -> zipcore::ListReport {
    list_zip(ListRequest {
        archive: archive.to_path_buf(),
        encoding: None,
    })
    .unwrap_or_else(|e| panic!("list_zip {}: {e}", archive.display()))
}

#[test]
fn committed_fixtures_detect_per_section_3_4() {
    let cases = [
        ("cp932-typical.zip", NameEncoding::Cp932, 0.8, false),
        ("utf8-efs.zip", NameEncoding::Utf8, 0.95, false),
        ("utf8-finder.zip", NameEncoding::Utf8, 0.8, false),
        ("ambiguous-mixed.zip", NameEncoding::Cp932, 0.4, true),
    ];
    for (name, encoding, confidence, ambiguous) in cases {
        let zip = fixture_dir().join(name);
        assert!(zip.exists(), "committed fixture missing: {}", zip.display());
        let report = list_auto(&zip);
        assert_eq!(report.detection.encoding, encoding, "{name}");
        assert!(
            (report.detection.confidence - confidence).abs() < 1e-6,
            "{name} confidence {} != {confidence}",
            report.detection.confidence
        );
        assert_eq!(report.detection.ambiguous, ambiguous, "{name}");
        assert_eq!(report.encoding, encoding, "{name} effective");
        let expected = serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(fixture_dir().join(name.replace(".zip", ".json"))).unwrap(),
        )
        .unwrap();
        let want: Vec<String> = expected["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name_hex"].as_str().unwrap().to_string())
            .collect();
        let got: Vec<String> = report.entries.iter().map(|e| hex(&e.name_raw)).collect();
        assert_eq!(got, want, "{name} name_hex");
        for (e, exp) in report
            .entries
            .iter()
            .zip(expected["entries"].as_array().unwrap())
        {
            assert_eq!(e.gpbf, exp["gpbf"].as_u64().unwrap() as u16, "{name} gpbf");
        }
    }
}

#[test]
fn override_changes_display_name_on_cp932_zip() {
    let zip = fixture_dir().join("cp932-typical.zip");
    let auto = list_auto(&zip);
    assert_eq!(auto.encoding, NameEncoding::Cp932);
    let ga = auto
        .entries
        .iter()
        .find(|e| e.name_raw == [0x82, 0xAA, 0x2E, 0x74, 0x78, 0x74])
        .expect("が.txt raw");
    assert_eq!(ga.display_name, "が.txt");
    assert_ne!(ga.name_raw, "が.txt".as_bytes());

    let forced = list_zip(ListRequest {
        archive: zip,
        encoding: Some(NameEncoding::Utf8),
    })
    .unwrap();
    assert_eq!(forced.encoding, NameEncoding::Utf8);
    assert_eq!(forced.detection.encoding, NameEncoding::Cp932);
    let ga_forced = forced
        .entries
        .iter()
        .find(|e| e.name_raw == [0x82, 0xAA, 0x2E, 0x74, 0x78, 0x74])
        .unwrap();
    assert_ne!(ga_forced.display_name, ga.display_name);
    assert_ne!(ga_forced.display_name, "が.txt");
}

#[test]
fn override_utf8_zip_to_cp932_changes_display_name() {
    let zip = fixture_dir().join("utf8-efs.zip");
    let auto = list_auto(&zip);
    let ga = auto
        .entries
        .iter()
        .find(|e| e.name_raw == "が.txt".as_bytes())
        .unwrap();
    assert_eq!(ga.display_name, "が.txt");
    assert_eq!(ga.gpbf & EFS_UTF8_BIT, EFS_UTF8_BIT);

    let forced = list_zip(ListRequest {
        archive: zip,
        encoding: Some(NameEncoding::Cp932),
    })
    .unwrap();
    let ga_forced = forced
        .entries
        .iter()
        .find(|e| e.name_raw == "が.txt".as_bytes())
        .unwrap();
    assert_ne!(ga_forced.display_name, "が.txt");
}

#[test]
fn extract_cp932_fixture_writes_unicode_names_and_bytes() {
    let tmp = support::TempDir::new();
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    extract_zip(
        extract_req(fixture_dir().join("cp932-typical.zip"), dest.clone(), false),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(fs::read(dest.join("が.txt")).unwrap(), b"ga\n");
    assert_eq!(fs::read(dest.join("ドキュメント.txt")).unwrap(), b"docs\n");
    assert_eq!(fs::read(dest.join("readme.txt")).unwrap(), b"ascii\n");
}

#[test]
fn extract_utf8_efs_fixture_writes_unicode_names_and_bytes() {
    let tmp = support::TempDir::new();
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    extract_zip(
        extract_req(fixture_dir().join("utf8-efs.zip"), dest.clone(), false),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(fs::read(dest.join("が.txt")).unwrap(), b"ga\n");
    assert_eq!(fs::read(dest.join("ドキュメント.txt")).unwrap(), b"docs\n");
}

fn assert_extract_traversal(name: &[u8]) {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("evil.zip");
    write_stored_zip(&zip, &[(name, b"pwned", 0)]);
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    let err = extract_zip(
        extract_req(zip.clone(), dest.clone(), false),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(err, ZipcoreError::PathTraversal { .. }),
        "name {} err={err}",
        hex(name)
    );
    assert!(!dest.join("evil").exists());
    let list_err = list_zip(ListRequest {
        archive: zip,
        encoding: None,
    })
    .unwrap_err();
    assert!(matches!(list_err, ZipcoreError::PathTraversal { .. }));
}

#[test]
fn extract_rejects_dotdot() {
    assert_extract_traversal(b"../evil");
}

#[test]
fn extract_rejects_absolute_unix() {
    assert_extract_traversal(b"/tmp/evil");
}

#[test]
fn extract_rejects_absolute_backslash() {
    assert_extract_traversal(br"\Windows\evil");
}

#[test]
fn extract_rejects_drive_letter() {
    assert_extract_traversal(b"C:/evil");
    assert_extract_traversal(br"C:\evil");
    assert_extract_traversal(b"C:evil");
}

#[test]
fn extract_rejects_backslash_escape() {
    assert_extract_traversal(br"foo\..\evil");
    assert_extract_traversal(b"foo/../evil");
}

#[test]
fn extract_rejects_nul() {
    assert_extract_traversal(b"ok\0evil");
}

#[test]
fn extract_rejects_empty_and_dot_only_names() {
    assert_extract_traversal(b"");
    assert_extract_traversal(b".");
    assert_extract_traversal(b"././.");
}

#[test]
fn extract_rejects_file_vs_dir_collision() {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("c.zip");
    write_stored_zip(
        &zip,
        &[(b"foo/bar.txt", b"nested", 0), (b"foo", b"file", 0)],
    );
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    let err = extract_zip(
        extract_req(zip.clone(), dest.clone(), false),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ZipcoreError::Corrupt(_) | ZipcoreError::PathTraversal { .. }
        ),
        "{err}"
    );
    assert!(!dest.join("foo").exists() || dest.join("foo").is_dir());
    let list_err = list_zip(ListRequest {
        archive: zip,
        encoding: None,
    })
    .unwrap_err();
    assert!(
        matches!(
            list_err,
            ZipcoreError::Corrupt(_) | ZipcoreError::PathTraversal { .. }
        ),
        "{list_err}"
    );
}

#[test]
fn extract_refuses_to_replace_directory_with_file() {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("c.zip");
    write_stored_zip(&zip, &[(b"foo", b"file", 0)]);
    let dest = tmp.path().join("out");
    fs::create_dir_all(dest.join("foo")).unwrap();
    fs::write(dest.join("foo").join("keep.txt"), b"keep").unwrap();
    let err = extract_zip(
        extract_req(zip, dest.clone(), true),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ZipcoreError::Corrupt(_) | ZipcoreError::OverwriteRequired { .. }
        ),
        "{err}"
    );
    assert_eq!(
        fs::read(dest.join("foo").join("keep.txt")).unwrap(),
        b"keep"
    );
}

#[test]
fn extract_rejects_unc() {
    assert_extract_traversal(br"\\server\share\evil");
    assert_extract_traversal(b"//server/share/evil");
}

#[test]
fn extract_rejects_symlink_entry() {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("link.zip");
    // Unix host + symlink mode in the high 16 bits of external attributes.
    write_stored_zip_ex(&zip, &[(b"link", b"../evil", 0)], 3, 0o120777 << 16);
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    let err = extract_zip(
        extract_req(zip, dest.clone(), false),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(err, ZipcoreError::PathTraversal { .. }), "{err}");
    assert!(!dest.join("link").exists());
}

#[test]
fn overwrite_false_leaves_existing_and_errors() {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("a.zip");
    write_stored_zip(&zip, &[(b"keep.txt", b"new", 0)]);
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("keep.txt"), b"old").unwrap();
    let err = extract_zip(
        extract_req(zip, dest.clone(), false),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(err, ZipcoreError::OverwriteRequired { .. }),
        "{err}"
    );
    assert_eq!(fs::read(dest.join("keep.txt")).unwrap(), b"old");
}

#[test]
fn overwrite_true_replaces_existing() {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("a.zip");
    write_stored_zip(&zip, &[(b"keep.txt", b"new", 0)]);
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("keep.txt"), b"old").unwrap();
    extract_zip(
        extract_req(zip, dest.clone(), true),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(fs::read(dest.join("keep.txt")).unwrap(), b"new");
    let extras: Vec<String> = fs::read_dir(&dest)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(extras, vec!["keep.txt".to_string()]);
}

#[test]
fn cancel_after_overwrite_restores_original() {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("a.zip");
    write_stored_zip(
        &zip,
        &[(b"keep.txt", b"new-bytes", 0), (b"two.txt", b"second", 0)],
    );
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("keep.txt"), b"old").unwrap();
    let cancel = AtomicBool::new(false);
    let err = extract_zip(
        extract_req(zip, dest.clone(), true),
        |p| {
            if p.phase == ProgressPhase::Writing && p.entries_done >= 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    )
    .unwrap_err();
    assert!(matches!(err, ZipcoreError::Cancelled), "{err}");
    assert_eq!(fs::read(dest.join("keep.txt")).unwrap(), b"old");
    assert!(!dest.join("two.txt").exists());
}

#[test]
fn cancel_during_extract_removes_partial_files() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), b"aaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
    fs::write(src.join("b.txt"), b"bbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
    let zip = tmp.path().join("out.zip");
    write_zip(
        WriteRequest {
            inputs: vec![
                InputItem {
                    abs_path: src.join("a.txt"),
                    as_dir: false,
                },
                InputItem {
                    abs_path: src.join("b.txt"),
                    as_dir: false,
                },
            ],
            dest: zip.clone(),
            password: None,
            aes: AesMode::Aes256,
            split_bytes: None,
            replacements: Default::default(),
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();

    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    let cancel = AtomicBool::new(false);
    let err = extract_zip(
        extract_req(zip, dest.clone(), false),
        |p| {
            if p.phase == ProgressPhase::Writing && p.entries_done >= 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    )
    .unwrap_err();
    assert!(matches!(err, ZipcoreError::Cancelled), "{err}");
    let leftovers: Vec<_> = fs::read_dir(&dest)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        leftovers.is_empty(),
        "cancel must drop partial extract: {leftovers:?}"
    );
}

#[test]
fn first_progress_is_starting() {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("a.zip");
    write_stored_zip(&zip, &[(b"a.txt", b"hi", 0)]);
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    let phases = Mutex::new(Vec::new());
    extract_zip(
        extract_req(zip, dest, false),
        |p| phases.lock().unwrap().push(p.phase),
        &AtomicBool::new(false),
    )
    .unwrap();
    let phases = phases.lock().unwrap();
    assert_eq!(phases.first().copied(), Some(ProgressPhase::Starting));
    assert_eq!(phases.last().copied(), Some(ProgressPhase::Done));
}

#[test]
fn password_is_not_implemented() {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("a.zip");
    write_stored_zip(&zip, &[(b"a.txt", b"hi", 0)]);
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    let err = extract_zip(
        ExtractRequest {
            archive: zip,
            dest,
            encoding: None,
            overwrite: false,
            password: Some(SecretString::new("secret")),
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(err, ZipcoreError::AesNotImplemented));
}

#[test]
fn crc_mismatch_is_corrupt_and_leaves_no_file() {
    let tmp = support::TempDir::new();
    let zip = tmp.path().join("bad.zip");
    write_stored_zip(&zip, &[(b"a.txt", b"hello", 0)]);
    let mut bytes = fs::read(&zip).unwrap();
    // ローカルヘッダ 30 + 名前 "a.txt" の直後がペイロード。CRC はそのまま残す。
    let data_at = 30 + b"a.txt".len();
    bytes[data_at] ^= 0xFF;
    fs::write(&zip, bytes).unwrap();
    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    let err = extract_zip(
        extract_req(zip, dest.clone(), false),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(err, ZipcoreError::Corrupt(_)) || matches!(err, ZipcoreError::Io(_)),
        "{err}"
    );
    assert!(!dest.join("a.txt").exists());
}

#[test]
fn extract_write_zip_roundtrip_bytes() {
    let tmp = support::TempDir::new();
    let src = tmp.path().join("src");
    fs::create_dir_all(src.join("書類")).unwrap();
    fs::write(src.join("が.txt"), b"ga-bytes").unwrap();
    fs::write(
        src.join("書類").join("こんにちは.txt"),
        "こんにちは".as_bytes(),
    )
    .unwrap();
    let zip = tmp.path().join("out.zip");
    write_zip(
        WriteRequest {
            inputs: vec![
                InputItem {
                    abs_path: src.join("が.txt"),
                    as_dir: false,
                },
                InputItem {
                    abs_path: src.join("書類"),
                    as_dir: true,
                },
            ],
            dest: zip.clone(),
            password: None,
            aes: AesMode::Aes256,
            split_bytes: None,
            replacements: Default::default(),
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    let listed = list_auto(&zip);
    assert_eq!(listed.detection.encoding, NameEncoding::Cp932);
    assert!(listed.entries.iter().any(|e| e.display_name == "が.txt"));
    assert!(listed
        .entries
        .iter()
        .any(|e| e.display_name == "書類/こんにちは.txt"));

    let dest = tmp.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    extract_zip(
        extract_req(zip, dest.clone(), false),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(fs::read(dest.join("が.txt")).unwrap(), b"ga-bytes");
    assert_eq!(
        fs::read(dest.join("書類").join("こんにちは.txt")).unwrap(),
        "こんにちは".as_bytes()
    );
}
