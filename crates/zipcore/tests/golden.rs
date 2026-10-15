//! ゴールデン: `tests/golden/*/src` を write_zip し、name_hex / gpbf / method を expected.json と照合する。

mod support;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;
use zipcore::{
    extract_zip, inspect_zip_headers, plan_write, write_zip, AesMode, ExtractRequest, InputItem,
    WriteRequest, EFS_UTF8_BIT,
};

#[derive(Debug, Deserialize)]
struct Expected {
    entries: Vec<ExpectedEntry>,
    #[serde(default)]
    excluded: Vec<String>,
    #[serde(default)]
    replacements: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ExpectedEntry {
    name_hex: String,
    gpbf: u16,
    method: u16,
    #[serde(default)]
    name_nfc: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Meta {
    #[serde(default)]
    nfd_files: Vec<NfdFile>,
    #[serde(default)]
    empty_dirs: Vec<String>,
    #[serde(default)]
    extra_files: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct NfdFile {
    name_nfc: String,
    #[serde(default)]
    content: String,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// PowerShell 5.1 は `-File \\?\D:\...` だと `$PSScriptRoot` が空になり、
/// `Join-Path` が `drive is null` で落ちる。拡張プレフィックスを外す。
fn win_cmd_path(path: &Path) -> PathBuf {
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

fn golden_root() -> PathBuf {
    workspace_root().join("tests/golden")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn norm_hex(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

struct Case {
    dir: PathBuf,
    expected: Expected,
}

fn load_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let root = golden_root();
    let mut dirs: Vec<_> = fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("tests/golden がありません: {e}"))
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let expected_path = dir.join("expected.json");
        if !expected_path.exists() {
            continue;
        }
        let text = fs::read_to_string(&expected_path).unwrap();
        let expected: Expected = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{}: {e}", expected_path.display()));
        cases.push(Case { dir, expected });
    }
    assert!(!cases.is_empty(), "golden cases missing");
    cases
}

fn materialize(case: &Case) -> support::TempDir {
    let tmp = support::TempDir::new();
    let payload = tmp.path().join("payload");
    fs::create_dir_all(&payload).unwrap();
    let src = case.dir.join("src");
    if src.exists() {
        copy_tree(&src, &payload);
    }
    let meta_path = case.dir.join("meta.json");
    let meta: Meta = if meta_path.exists() {
        serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap()
    } else {
        Meta::default()
    };
    for dir in &meta.empty_dirs {
        fs::create_dir_all(payload.join(dir)).unwrap();
    }
    for (rel, body) in &meta.extra_files {
        let path = payload.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body.as_bytes()).unwrap();
    }
    for nfd in &meta.nfd_files {
        let name: String = nfd.name_nfc.nfd().collect();
        let path = payload.join(&name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, nfd.content.as_bytes()).unwrap();
    }
    tmp
}

fn copy_tree(from: &Path, to: &Path) {
    for ent in fs::read_dir(from).unwrap() {
        let ent = ent.unwrap();
        let dest = to.join(ent.file_name());
        if ent.file_type().unwrap().is_dir() {
            fs::create_dir_all(&dest).unwrap();
            copy_tree(&ent.path(), &dest);
        } else {
            fs::copy(ent.path(), dest).unwrap();
        }
    }
}

fn case_inputs(payload: &Path) -> Vec<InputItem> {
    let mut inputs = Vec::new();
    for ent in fs::read_dir(payload).unwrap() {
        let ent = ent.unwrap();
        inputs.push(InputItem {
            abs_path: ent.path(),
            as_dir: ent.file_type().unwrap().is_dir(),
        });
    }
    inputs
}

fn write_case(case: &Case, payload: &Path, dest: &Path) -> zipcore::WriteReport {
    let req = WriteRequest {
        inputs: case_inputs(payload),
        dest: dest.to_path_buf(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: case.expected.replacements.clone(),
    };
    write_zip(req, |_| {}, &AtomicBool::new(false)).unwrap_or_else(|e| {
        panic!("{} write_zip failed: {e}", case.dir.display());
    })
}

fn assert_headers(case: &Case, zip: &Path) {
    let entries = inspect_zip_headers(zip).unwrap_or_else(|e| {
        panic!("{} inspect: {e}", case.dir.display());
    });
    let got: HashSet<(String, u16, u16)> = entries
        .iter()
        .map(|e| (hex(&e.name_raw), e.gpbf, e.method))
        .collect();
    let want: HashSet<(String, u16, u16)> = case
        .expected
        .entries
        .iter()
        .map(|e| (norm_hex(&e.name_hex), e.gpbf, e.method))
        .collect();
    assert_eq!(
        got,
        want,
        "{} header set mismatch\ngot: {got:?}\nwant: {want:?}",
        case.dir.file_name().unwrap().to_string_lossy()
    );
    for e in &entries {
        assert_eq!(e.gpbf & EFS_UTF8_BIT, 0);
        assert_eq!(e.gpbf & 1, 0);
        assert_eq!(e.host_os, 0);
        assert!(!e.has_unicode_path_extra());
        assert!(e.extra_local.is_empty() && e.extra_central.is_empty());
    }
    for e in &case.expected.entries {
        assert_eq!(e.gpbf & EFS_UTF8_BIT, 0, "expected.json gpbf bit 11");
    }
    let _ = &case.expected.excluded;
}

fn python_bin() -> Option<&'static str> {
    for bin in ["python3", "python"] {
        if Command::new(bin)
            .arg("-c")
            .arg("import sys; sys.exit(0)")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Some(bin);
        }
    }
    None
}

#[test]
fn win_cmd_path_strips_verbatim_prefix() {
    assert_eq!(
        win_cmd_path(Path::new(
            r"\\?\D:\a\winzip-jp\tests\oracles\windows-extract.ps1"
        )),
        PathBuf::from(r"D:\a\winzip-jp\tests\oracles\windows-extract.ps1")
    );
    assert_eq!(
        win_cmd_path(Path::new(r"\\?\UNC\server\share\x.ps1")),
        PathBuf::from(r"\\server\share\x.ps1")
    );
}

#[test]
fn windows_extract_ps1_is_utf8_bom_ascii() {
    let bytes = fs::read(workspace_root().join("tests/oracles/windows-extract.ps1"))
        .expect("windows-extract.ps1");
    assert!(
        bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "UTF-8 BOM is required so Windows PowerShell 5.1 does not parse this as CP1252"
    );
    let body = &bytes[3..];
    assert!(
        body.is_ascii(),
        "ps1 source must stay ASCII; non-ASCII in 5.1 without a BOM eats string terminators"
    );
    let text = std::str::from_utf8(body).unwrap();
    assert!(
        text.contains(r"'C:\Program Files\7-Zip\7z.exe'"),
        "7-Zip path must be a single-quoted literal"
    );
    assert!(
        text.contains(r#"& "$Seven""#),
        "7-Zip invocation must quote the executable path"
    );
}

#[test]
fn golden_headers_match_expected_json() {
    for case in load_cases() {
        let tmp = materialize(&case);
        let payload = tmp.path().join("payload");
        let zip = tmp.path().join("out.zip");
        let report = write_case(&case, &payload, &zip);
        assert_headers(&case, &zip);
        let got_ex: HashSet<_> = report.excluded.into_iter().collect();
        let want_ex: HashSet<_> = case.expected.excluded.iter().cloned().collect();
        assert_eq!(
            got_ex,
            want_ex,
            "{} excluded mismatch",
            case.dir.file_name().unwrap().to_string_lossy()
        );
    }
}

#[test]
fn golden_inspect_zip_py_matches() {
    let py = python_bin().expect("python が必要です");
    let script = workspace_root().join("tests/oracles/inspect_zip.py");
    assert!(script.exists(), "{}", script.display());
    for case in load_cases() {
        let tmp = materialize(&case);
        let payload = tmp.path().join("payload");
        let zip = tmp.path().join("out.zip");
        write_case(&case, &payload, &zip);
        let expected = case.dir.join("expected.json");
        let out = Command::new(py)
            .arg(&script)
            .arg(&zip)
            .arg("--expect")
            .arg(&expected)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{} inspect_zip.py failed\nstdout:\n{}\nstderr:\n{}",
            case.dir.file_name().unwrap().to_string_lossy(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[cfg(windows)]
#[test]
#[ignore = "windows-oracle job: 7-Zip -mcp=932 と補助レポート"]
fn windows_oracle_7zip_and_optional_reports() {
    let ps1 = workspace_root().join("tests/oracles/windows-extract.ps1");
    assert!(ps1.exists(), "{}", ps1.display());
    for case in load_cases() {
        let tmp = materialize(&case);
        let payload = tmp.path().join("payload");
        let zip = tmp.path().join("out.zip");
        write_case(&case, &payload, &zip);
        let work = tmp.path().join("oracle-work");
        fs::create_dir_all(&work).unwrap();
        let out = Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(win_cmd_path(&ps1))
            .arg("-ZipPath")
            .arg(win_cmd_path(&zip))
            .arg("-ExpectPath")
            .arg(win_cmd_path(&case.dir.join("expected.json")))
            .arg("-WorkDir")
            .arg(win_cmd_path(&work))
            .output()
            .expect("powershell");
        assert!(
            out.status.success(),
            "{} windows-extract.ps1 failed\nstdout:\n{}\nstderr:\n{}",
            case.dir.file_name().unwrap().to_string_lossy(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[cfg(not(windows))]
#[test]
#[ignore = "windows-oracle job は windows-2022 専用"]
fn windows_oracle_7zip_and_optional_reports() {
    panic!("windows-oracle は windows-2022 で実行する");
}

#[test]
fn golden_extract_bytes_match_source() {
    for case in load_cases() {
        let tmp = materialize(&case);
        let payload = tmp.path().join("payload");
        let zip = tmp.path().join("out.zip");
        write_case(&case, &payload, &zip);
        let plan = plan_write(&WriteRequest {
            inputs: case_inputs(&payload),
            dest: zip.clone(),
            password: None,
            aes: AesMode::Aes256,
            split_bytes: None,
            replacements: case.expected.replacements.clone(),
        })
        .unwrap_or_else(|e| panic!("{} plan: {e}", case.dir.display()));
        let dest = tmp.path().join("extracted");
        fs::create_dir_all(&dest).unwrap();
        extract_zip(
            ExtractRequest {
                archive: zip,
                dest: dest.clone(),
                encoding: None,
                overwrite: false,
                password: None,
            },
            |_| {},
            &AtomicBool::new(false),
        )
        .unwrap_or_else(|e| panic!("{} extract: {e}", case.dir.display()));
        for entry in &plan.entries {
            let rel = entry.relative_path.trim_end_matches('/');
            let got_path = dest.join(rel);
            if entry.is_dir {
                assert!(
                    got_path.is_dir(),
                    "{} missing dir {}",
                    case.dir.file_name().unwrap().to_string_lossy(),
                    entry.relative_path
                );
                continue;
            }
            let got = fs::read(&got_path).unwrap_or_else(|e| {
                panic!(
                    "{} read {}: {e}",
                    case.dir.file_name().unwrap().to_string_lossy(),
                    got_path.display()
                )
            });
            let want = fs::read(&entry.source).unwrap();
            assert_eq!(
                got,
                want,
                "{} content mismatch {}",
                case.dir.file_name().unwrap().to_string_lossy(),
                entry.relative_path
            );
        }
    }
}

#[test]
fn expected_json_name_nfc_is_present_for_oracle() {
    for case in load_cases() {
        for e in &case.expected.entries {
            assert!(
                e.name_nfc.as_ref().map(|s| !s.is_empty()).unwrap_or(false),
                "{} missing name_nfc",
                case.dir.display()
            );
        }
    }
}
