//! entitlements が設計 §4.1 と一致し、fs scope に $HOME を含めない。

use std::fs;
use std::path::{Path, PathBuf};

fn tauri_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{} を読めません: {e}", path.display()))
}

fn plist_entries(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<key>") {
        rest = &rest[start + 5..];
        let Some(end) = rest.find("</key>") else {
            break;
        };
        let key = rest[..end].trim().to_string();
        rest = &rest[end + 6..];
        let value = if rest.trim_start().starts_with("<true/>") {
            "true".into()
        } else if rest.trim_start().starts_with("<false/>") {
            "false".into()
        } else if let Some(s) = rest.find("<string>") {
            let rest = &rest[s + 8..];
            let end = rest.find("</string>").expect("string 終端");
            rest[..end].to_string()
        } else {
            String::new()
        };
        out.push((key, value));
    }
    out
}

#[test]
fn appstore_entitlements_match_design_section_4_1() {
    let plist = read(&tauri_dir().join("Entitlements.plist"));
    let entries = plist_entries(&plist);
    assert_eq!(
        entries,
        vec![
            ("com.apple.security.app-sandbox".into(), "true".into()),
            (
                "com.apple.security.files.user-selected.read-write".into(),
                "true".into()
            ),
            (
                "com.apple.security.files.bookmarks.app-scope".into(),
                "true".into()
            ),
            (
                "com.apple.application-identifier".into(),
                "$TEAM_ID.jp.riku.winzip".into()
            ),
            (
                "com.apple.developer.team-identifier".into(),
                "$TEAM_ID".into()
            ),
        ]
    );
    for banned in [
        "downloads.read-write",
        "network.client",
        "network.server",
        "temporary-exception.files.home-relative-path",
        "files.user-selected.read-only",
        "files.bookmarks.document-scope",
    ] {
        assert!(
            !plist.contains(banned),
            "禁止 entitlement が含まれている: {banned}"
        );
    }

    let appstore = read(&tauri_dir().join("tauri.appstore.conf.json"));
    let json: serde_json::Value = serde_json::from_str(&appstore).unwrap();
    let entitlements = json["bundle"]["macOS"]["entitlements"]
        .as_str()
        .expect("tauri.appstore.conf.json が entitlements を読む");
    assert!(
        entitlements.ends_with("Entitlements.plist"),
        "{entitlements}"
    );
    assert!(
        json.get("plugins").is_none() || json["plugins"].get("persisted-scope").is_none(),
        "persisted-scope を付けない"
    );
}

#[test]
fn fs_scope_does_not_include_home() {
    let mut files = vec![
        tauri_dir().join("tauri.conf.json"),
        tauri_dir().join("tauri.appstore.conf.json"),
        tauri_dir().join("capabilities/default.json"),
        tauri_dir().join("Cargo.toml"),
        tauri_dir().join("Entitlements.plist"),
    ];
    if let Ok(entries) = fs::read_dir(tauri_dir().join("capabilities")) {
        for e in entries.filter_map(|e| e.ok()) {
            files.push(e.path());
        }
    }
    for path in files {
        if !path.exists() {
            continue;
        }
        let text = read(&path);
        assert!(
            !text.contains("$HOME/**") && !text.contains("$HOME/"),
            "{} に $HOME スコープがある",
            path.display()
        );
        assert!(
            !text.contains("persisted-scope") && !text.contains("persisted_scope"),
            "{} が persisted-scope を参照している",
            path.display()
        );
        assert!(
            !text.contains("downloads.read-write"),
            "{} に downloads.read-write がある",
            path.display()
        );
    }
}

#[test]
fn cargo_does_not_depend_on_persisted_scope() {
    let toml = read(&tauri_dir().join("Cargo.toml"));
    assert!(!toml.contains("tauri-plugin-persisted-scope"));
    assert!(toml.contains("tauri-plugin-dialog"));
    assert!(toml.contains("tauri-plugin-fs"));
}
