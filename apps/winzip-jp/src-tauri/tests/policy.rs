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
    assert!(
        appstore.contains("Entitlements.plist"),
        "提出用設定が entitlements を指していない"
    );
    assert!(
        !appstore.contains("wdio") && !appstore.contains("e2e"),
        "提出用設定に Webdriver を入れない"
    );
}

#[test]
fn fs_scope_does_not_include_home() {
    let mut files = vec![
        tauri_dir().join("tauri.conf.json"),
        tauri_dir().join("tauri.appstore.conf.json"),
        tauri_dir().join("tauri.e2e.conf.json"),
        tauri_dir().join("capabilities/default.json"),
        tauri_dir().join("Cargo.toml"),
        tauri_dir().join("Entitlements.plist"),
        tauri_dir().join("Info.plist"),
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
        if path.file_name().is_some_and(|n| n == "default.json") {
            assert!(
                !text.contains("$TEMP/**") && !text.contains("fs:default") && !text.contains("fs:scope"),
                "{} に静的な fs スコープがある",
                path.display()
            );
        }
    }
}

#[test]
fn capabilities_are_pinned_to_default() {
    for name in ["tauri.conf.json", "tauri.appstore.conf.json", "tauri.e2e.conf.json"] {
        let json: serde_json::Value =
            serde_json::from_str(&read(&tauri_dir().join(name))).expect(name);
        assert_eq!(
            json["app"]["security"]["capabilities"],
            serde_json::json!(["default"]),
            "{name} の capabilities が default に固定されていない"
        );
    }
    assert!(
        !tauri_dir().join("capabilities/e2e.json").exists(),
        "e2e.json は全ビルドに載るので置かない"
    );
    let cargo = read(&tauri_dir().join("Cargo.toml"));
    assert!(
        cargo.contains("e2e = []"),
        "e2e feature が default に入っていないこと"
    );
    assert!(
        !cargo.contains("default = [\"e2e\"]") && !cargo.contains("default = [\"e2e\""),
        "e2e を default feature にしない"
    );
    let mut names: Vec<String> = fs::read_dir(tauri_dir().join("capabilities"))
        .expect("capabilities")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["default.json".to_string()]);
}

#[test]
fn cargo_does_not_depend_on_persisted_scope() {
    let toml = read(&tauri_dir().join("Cargo.toml"));
    assert!(!toml.contains("tauri-plugin-persisted-scope"));
    assert!(toml.contains("tauri-plugin-dialog"));
    assert!(toml.contains("tauri-plugin-fs"));
}

#[test]
fn info_plist_sets_encryption_and_utility_category() {
    let plist = read(&tauri_dir().join("Info.plist"));
    let entries = plist_entries(&plist);
    assert!(
        entries
            .iter()
            .any(|(k, v)| k == "ITSAppUsesNonExemptEncryption" && v == "true"),
        "AES-256 があるので非免除の暗号化あり: {entries:?}"
    );
    assert!(
        entries.iter().any(|(k, v)| {
            k == "LSApplicationCategoryType" && v == "public.app-category.utilities"
        }),
        "{entries:?}"
    );
    assert!(
        plist.contains("<key>NSIsRelatedItemType</key>") && plist.contains("<string>z01</string>"),
        "分割ピースを related item として宣言する"
    );
    assert!(
        !plist.contains("downloads.read-write") && !plist.contains("network.client"),
        "Info.plist に禁止 entitlement を書かない"
    );

    for name in ["tauri.conf.json", "tauri.appstore.conf.json"] {
        let json: serde_json::Value =
            serde_json::from_str(&read(&tauri_dir().join(name))).expect(name);
        assert_eq!(
            json["bundle"]["category"].as_str(),
            Some("Utility"),
            "{name} の category"
        );
        let info = json["bundle"]["macOS"]["infoPlist"]
            .as_str()
            .expect("infoPlist");
        assert!(info.ends_with("Info.plist"), "{name}: {info}");
    }
}

#[test]
fn e2e_open_is_cfg_gated_in_source() {
    let lib = read(&tauri_dir().join("src/lib.rs"));
    assert!(
        lib.contains("cfg(all(debug_assertions, feature = \"e2e\"))"),
        "--e2e-open を App Store ビルドに入れない"
    );
    assert!(
        !lib.contains("e2e_open::parse") || lib.contains("feature = \"e2e\""),
        "e2e_open の呼び出しが feature で囲まれていること"
    );
    let open = read(&tauri_dir().join("src/e2e_open.rs"));
    assert!(open.contains("--e2e-open"));
    assert!(open.contains("--e2e-deny"));
    assert!(open.contains("--e2e-report"));
}
