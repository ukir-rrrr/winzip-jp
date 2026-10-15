//! ダイアログを使わずパス直指定で create / list / extract を通す。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use winzip_jp_lib::{
    create_archive_direct, extract_archive_direct, list_archive_direct, CreateDto, ExtractDto,
    Sandbox, PROGRESS_EVENT,
};
use zipcore::{inspect_zip_headers, NameEncoding, Progress, ProgressPhase, EFS_UTF8_BIT};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("workspace root")
}

fn unique_dir(label: &str) -> PathBuf {
    // コンテナ tmp 接頭辞 `winzip-jp-` とは別名。grant 経路を本物に近づける。
    let path = std::env::temp_dir().join(format!(
        "wzjp-cmd-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn grant(paths: &[&Path]) -> Sandbox {
    let mut sandbox = Sandbox::new();
    sandbox.grant_many(paths);
    sandbox
}

#[test]
fn progress_event_name_is_stable() {
    assert_eq!(PROGRESS_EVENT, "zip://progress");
}

#[test]
fn create_list_extract_with_direct_paths() {
    let root = unique_dir("cle");
    let input = root.join("書類");
    fs::create_dir_all(&input).unwrap();
    let src = input.join("こんにちは.txt");
    fs::write(&src, b"hello").unwrap();
    let dest = root.join("out.zip");
    let extract_to = root.join("out");
    fs::create_dir_all(&extract_to).unwrap();

    let sandbox = grant(&[&root, &input, &src, &dest, &extract_to]);
    let cancel = AtomicBool::new(false);
    let report = create_archive_direct(
        &sandbox,
        &CreateDto {
            paths: vec![input.to_string_lossy().into_owned()],
            replacements: Default::default(),
        },
        dest.to_str().unwrap(),
        |_| {},
        &cancel,
    )
    .expect("create");
    assert!(Path::new(&report.dest).exists());
    assert_eq!(
        fs::canonicalize(&report.dest).expect("report dest"),
        fs::canonicalize(&dest).expect("dest")
    );

    let headers = inspect_zip_headers(&dest).unwrap();
    let hello = headers
        .iter()
        .find(|e| e.name_raw.ends_with(b".txt"))
        .expect("txt entry");
    assert_eq!(
        hex(&hello.name_raw),
        "8F9197DE2F82B182F182C982BF82CD2E747874"
    );
    assert_eq!(hello.gpbf & EFS_UTF8_BIT, 0);

    let listed = list_archive_direct(&sandbox, dest.to_str().unwrap(), None).expect("list");
    assert_eq!(listed.encoding, NameEncoding::Cp932);
    let names: Vec<_> = listed
        .entries
        .iter()
        .map(|e| e.display_name.as_str())
        .collect();
    assert!(
        names.iter().any(|n| *n == "書類/こんにちは.txt"),
        "{names:?}"
    );
    assert!(listed
        .entries
        .iter()
        .all(|e| e.gpbf & EFS_UTF8_BIT == 0 && !e.name_hex.is_empty()));

    extract_archive_direct(
        &sandbox,
        &ExtractDto {
            archive: dest.to_string_lossy().into_owned(),
            dest: extract_to.to_string_lossy().into_owned(),
            encoding: Some(NameEncoding::Cp932),
            overwrite: false,
        },
        |_| {},
        &cancel,
    )
    .expect("extract");
    let extracted = extract_to.join("書類").join("こんにちは.txt");
    assert_eq!(fs::read(&extracted).unwrap(), b"hello");

    assert!(dest.exists());
    assert!(!dest.with_extension("zip.partial").exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn list_encoding_override_changes_display_name_only() {
    let zip = workspace_root().join("tests/fixtures/read/cp932-typical.zip");
    let mut sandbox = Sandbox::new();
    sandbox.grant(&zip);
    let auto = list_archive_direct(&sandbox, zip.to_str().unwrap(), None).unwrap();
    let utf8 =
        list_archive_direct(&sandbox, zip.to_str().unwrap(), Some(NameEncoding::Utf8)).unwrap();
    assert_eq!(auto.encoding, NameEncoding::Cp932);
    assert_eq!(utf8.encoding, NameEncoding::Utf8);
    assert_eq!(auto.entries.len(), utf8.entries.len());
    assert_eq!(auto.entries[0].name_hex, utf8.entries[0].name_hex);
    assert_eq!(auto.entries[0].name_hex, "82AA2E747874");
    assert_ne!(auto.entries[0].display_name, utf8.entries[0].display_name);
    assert_eq!(auto.entries[0].display_name, "が.txt");
}

#[test]
fn extract_writes_under_dest_not_container() {
    let root = unique_dir("extract-dest");
    let zip = workspace_root().join("tests/fixtures/read/cp932-typical.zip");
    let dest = root.join("展開");
    fs::create_dir_all(&dest).unwrap();
    let mut sandbox = Sandbox::new();
    sandbox.grant(&zip);
    sandbox.grant(&dest);
    sandbox.grant(&root);
    extract_archive_direct(
        &sandbox,
        &ExtractDto {
            archive: zip.to_string_lossy().into_owned(),
            dest: dest.to_string_lossy().into_owned(),
            encoding: Some(NameEncoding::Cp932),
            overwrite: false,
        },
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(dest.join("が.txt").exists() || dest.join("readme.txt").exists());
    assert!(
        !dest.join("cp932-typical.zip").exists(),
        "ZIP を展開先へ二重書きしていないこと"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn starting_progress_emits_within_two_seconds() {
    let root = unique_dir("progress");
    let src = root.join("a.txt");
    fs::write(&src, b"x").unwrap();
    let dest = root.join("a.zip");
    let sandbox = grant(&[&root, &src, &dest]);
    let (tx, rx) = mpsc::channel::<Progress>();
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let join = std::thread::spawn({
        let dest = dest.clone();
        move || {
            create_archive_direct(
                &sandbox,
                &CreateDto {
                    paths: vec![src.to_string_lossy().into_owned()],
                    replacements: Default::default(),
                },
                dest.to_str().unwrap(),
                |p| {
                    let _ = tx.send(p);
                },
                &cancel,
            )
        }
    });
    let first = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("2 秒以内に zip://progress の starting が必要");
    assert!(started.elapsed() <= Duration::from_secs(2));
    assert_eq!(first.phase, ProgressPhase::Starting);
    join.join().unwrap().expect("create");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ungranted_path_is_rejected() {
    let root = unique_dir("deny");
    let src = root.join("x.txt");
    fs::write(&src, b"x").unwrap();
    let dest = root.join("x.zip");
    let sandbox = Sandbox::new();
    let err = create_archive_direct(
        &sandbox,
        &CreateDto {
            paths: vec![src.to_string_lossy().into_owned()],
            replacements: Default::default(),
        },
        dest.to_str().unwrap(),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(err.contains("アクセスできません"), "{err}");
    assert!(!dest.exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cancel_does_not_leave_dest() {
    let root = unique_dir("cancel");
    let src_dir = root.join("many");
    fs::create_dir_all(&src_dir).unwrap();
    for i in 0..40 {
        fs::write(src_dir.join(format!("f{i}.txt")), vec![b'a'; 32 * 1024]).unwrap();
    }
    let dest = root.join("c.zip");
    let sandbox = grant(&[&root, &src_dir, &dest]);
    let cancel = AtomicBool::new(false);
    let err = create_archive_direct(
        &sandbox,
        &CreateDto {
            paths: vec![src_dir.to_string_lossy().into_owned()],
            replacements: Default::default(),
        },
        dest.to_str().unwrap(),
        |p| {
            if p.phase == ProgressPhase::Writing {
                cancel.store(true, Ordering::SeqCst);
            }
        },
        &cancel,
    )
    .unwrap_err();
    assert!(err.contains("キャンセル"), "{err}");
    assert!(!dest.exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn js_cannot_grant_ungranted_paths() {
    let root = unique_dir("js-grant");
    let secret = root.join("secret.txt");
    fs::write(&secret, b"nope").unwrap();
    let sandbox = Sandbox::new();
    let accepted =
        winzip_jp_lib::accepted_dropped_paths(&sandbox, &[secret.to_string_lossy().into_owned()])
            .unwrap();
    assert!(accepted.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn symlink_input_is_rejected_before_follow() {
    let root = unique_dir("symlink");
    let real = root.join("real.txt");
    let link = root.join("link.txt");
    let dest = root.join("out.zip");
    fs::write(&real, b"x").unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let sandbox = grant(&[&root, &real, &link, &dest]);
    let err = create_archive_direct(
        &sandbox,
        &CreateDto {
            paths: vec![link.to_string_lossy().into_owned()],
            replacements: Default::default(),
        },
        dest.to_str().unwrap(),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(err.contains("シンボリックリンク"), "{err}");
    assert!(!dest.exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_rejects_directory_as_dest() {
    let root = unique_dir("dest-dir");
    let src = root.join("a.txt");
    fs::write(&src, b"x").unwrap();
    let sandbox = grant(&[&root, &src]);
    let err = create_archive_direct(
        &sandbox,
        &CreateDto {
            paths: vec![src.to_string_lossy().into_owned()],
            replacements: Default::default(),
        },
        root.to_str().unwrap(),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(err.contains("フォルダ"), "{err}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn save_dest_grant_does_not_admit_siblings() {
    let root = unique_dir("save-sib");
    let src = root.join("ok.txt");
    let sibling = root.join("secret.txt");
    let dest = root.join("out.zip");
    fs::write(&src, b"ok").unwrap();
    fs::write(&sibling, b"no").unwrap();
    let mut sandbox = Sandbox::new();
    sandbox.grant(&src);
    sandbox.grant_save_dest(&dest);
    let err = create_archive_direct(
        &sandbox,
        &CreateDto {
            paths: vec![sibling.to_string_lossy().into_owned()],
            replacements: Default::default(),
        },
        dest.to_str().unwrap(),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(err.contains("アクセスできません"), "{err}");
    assert!(!dest.exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parent_escape_via_dotdot_is_rejected() {
    let root = unique_dir("escape");
    let allowed = root.join("allowed");
    let outside = root.join("outside.txt");
    fs::create_dir_all(&allowed).unwrap();
    fs::write(&outside, b"secret").unwrap();
    let sneak = allowed.join("..").join("outside.txt");
    let dest = allowed.join("x.zip");
    let mut sandbox = Sandbox::new();
    sandbox.grant(&allowed);
    let err = create_archive_direct(
        &sandbox,
        &CreateDto {
            paths: vec![sneak.to_string_lossy().into_owned()],
            replacements: Default::default(),
        },
        dest.to_str().unwrap(),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(err.contains("アクセスできません"), "{err}");
    let _ = fs::remove_dir_all(&root);
}
