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
                "com.apple.security.network.client".into(),
                "true".into()
            ),
            (
                "com.apple.application-identifier".into(),
                "CHSRTG92SZ.jp.riku.winzip".into()
            ),
            (
                "com.apple.developer.team-identifier".into(),
                "CHSRTG92SZ".into()
            ),
        ]
    );
    for banned in [
        "downloads.read-write",
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
    assert_eq!(
        json["bundle"]["macOS"]["signingIdentity"].as_str(),
        Some("Apple Distribution"),
        "提出用 signingIdentity は Apple Distribution（Team CHSRTG92SZ はスクリプトが解決）"
    );
    assert!(
        !plist.contains("$TEAM_ID"),
        "Entitlements.plist に $TEAM_ID プレースホルダを残さない"
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
fn csp_is_strict_and_not_null() {
    let json: serde_json::Value =
        serde_json::from_str(&read(&tauri_dir().join("tauri.conf.json"))).unwrap();
    let csp = json["app"]["security"]["csp"]
        .as_str()
        .expect("csp は文字列。null は XSS 対策を無効化する");
    assert!(
        csp.contains("default-src 'self'"),
        "default-src 'self' が無い: {csp}"
    );
    assert!(
        csp.contains("script-src 'self'"),
        "script-src 'self' が無い: {csp}"
    );
    assert!(
        csp.contains("style-src 'self' 'unsafe-inline'"),
        "style-src が無い: {csp}"
    );
    assert!(
        csp.contains("connect-src") && csp.contains("ipc:") && csp.contains("http://ipc.localhost"),
        "Tauri IPC の connect-src が無い: {csp}"
    );
    assert!(
        csp.contains("img-src 'self' data: asset:"),
        "img-src が無い: {csp}"
    );
    assert!(
        !csp.contains("unsafe-eval"),
        "script-src に unsafe-eval を足さない: {csp}"
    );
    assert!(
        !csp.split_whitespace().any(|t| t == "*"),
        "ワイルドカード CSP: {csp}"
    );
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
    assert!(
        entries
            .iter()
            .any(|(k, v)| k == "CFBundleDisplayName" && v == "文字化けしないZIP"),
        "Finder / Dock の表示名: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|(k, v)| k == "CFBundleName" && v == "文字化けしないZIP"),
        "メニューバーの短い名前: {entries:?}"
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
fn display_name_is_user_facing_and_identifier_unchanged() {
    let json: serde_json::Value =
        serde_json::from_str(&read(&tauri_dir().join("tauri.conf.json"))).unwrap();
    assert_eq!(
        json["productName"].as_str(),
        Some("文字化けしないZIP"),
        "productName はユーザー向け表示名"
    );
    assert_eq!(
        json["identifier"].as_str(),
        Some("jp.riku.winzip"),
        "Bundle ID は変えない"
    );
    assert_eq!(
        json["app"]["windows"][0]["title"].as_str(),
        Some("文字化けしないZIP")
    );
    let cargo = read(&tauri_dir().join("Cargo.toml"));
    assert!(
        cargo.contains("name = \"winzip-jp\""),
        "crate 名 winzip-jp は変えない"
    );
    assert!(
        !cargo.contains("name = \"文字化けしないZIP\""),
        "crate 名に表示名を入れない"
    );
}

#[test]
fn e2e_open_is_cfg_gated_in_source() {
    let lib = read(&tauri_dir().join("src/lib.rs"));
    assert!(
        lib.contains("cfg(all(debug_assertions, feature = \"e2e\"))"),
        "--e2e-open を App Store ビルドに入れない"
    );
    assert!(
        lib.contains("cfg(any(test, all(debug_assertions, feature = \"e2e\")))"),
        "probe を release / App Store に入れない"
    );
    assert!(
        lib.contains("e2e_open::maybe_run"),
        "e2e は maybe_run でヘッドレス終了する"
    );
    let open = read(&tauri_dir().join("src/e2e_open.rs"));
    assert!(open.contains("--e2e-open"));
    assert!(open.contains("--e2e-deny"));
    assert!(open.contains("--e2e-split-dest"));
    assert!(
        !open.contains("--e2e-report"),
        "レポートは stdout。サンドボックスはワークスペースへ書けない"
    );
    assert!(open.contains("maybe_run"), "不完全フラグで GUI に落とさない");
}

#[test]
fn sandbox_e2e_script_pins_app_and_strips_team_id_before_sign() {
    let sh = read(&tauri_dir().join("../scripts/sandbox-e2e.sh"));
    assert!(
        sh.contains("APP_NAME=\"文字化けしないZIP\"") && sh.contains("${APP_NAME}.app"),
        "バンドルパスを find | head にしない"
    );
    assert!(
        !sh.contains("送りZIP"),
        "旧表示名 送りZIP をサンドボックス起動に残さない"
    );
    assert!(
        !sh.contains("| head -1") && !sh.contains("| head -n 1"),
        "find | head で別 .app を拾わない"
    );
    assert!(
        sh.contains("data.pop(key")
            && sh.contains("grep -qF '$TEAM_ID'")
            && sh.contains("grep -qF 'CHSRTG92SZ'"),
        "署名前に team-identifier を剥がし、プレースホルダと実 Team ID の残存を落とす"
    );
    assert!(
        !sh.contains("if ! sign"),
        "TEAM_ID 除去を codesign 失敗後のフォールバックにしない"
    );
    assert!(
        !sh.contains("--e2e-report") && sh.contains("> \"$REPORT\""),
        "probe JSON は stdout リダイレクト。サンドボックスは成果物ディレクトリへ書けない"
    );
    assert!(
        !sh.contains("codesign --force --deep"),
        "codesign --deep は WebView ヘルパーを壊す"
    );
    let pkg = read(&tauri_dir().join("../scripts/appstore-pkg.sh"));
    assert!(pkg.contains("APP_NAME=\"文字化けしないZIP\"") && pkg.contains("${APP_NAME}.app"));
    assert!(!pkg.contains("送りZIP"));
    assert!(
        pkg.contains("CFBundleDisplayName"),
        "提出用 .app の表示名を Info.plist で確認する"
    );
    assert!(!pkg.contains("| head -1") && !pkg.contains("| head -n 1"));
    assert!(
        pkg.contains("Info.plist がありません"),
        "Info.plist 欠落をスキップしない"
    );
    assert!(
        !pkg.contains("if strings \"$BIN\" | grep"),
        "strings|grep の pipefail fail-open にしない"
    );
    assert!(
        pkg.contains("TEAM_ID=\"CHSRTG92SZ\"") && pkg.contains("BUNDLE_ID=\"jp.riku.winzip\""),
        "提出スクリプトに Team ID と Bundle ID を固定する"
    );
    assert!(
        pkg.contains("Apple Distribution")
            && pkg.contains("Mac Installer Distribution")
            && pkg.contains("3rd Party Mac Developer Installer")
            && pkg.contains("security find-identity -v 2>/dev/null || true"),
        "提出用証明書を Team ID で探し、一覧失敗で CI を落とさない"
    );
    assert!(
        pkg.contains("APPLE_SIGNING_IDENTITY")
            && pkg.contains(r#"{"bundle":{"macOS":{"signingIdentity":"-"}}}"#),
        "証明書が無い CI では signingIdentity を ad-hoc に落とす"
    );
    assert!(
        pkg.contains("APPSTORE_PROVISION_PROFILE") && pkg.contains("embedded.provisionprofile"),
        "プロファイルはローカルパスだけ。リポジトリには置かない"
    );
    assert!(
        pkg.contains("skip upload (--no-upload)")
            && !pkg.contains("iTMSTransporter")
            && !pkg.contains("altool"),
        "スクリプトは upload しない"
    );
    assert!(
        !pkg.contains("--deep"),
        "提出用 codesign にも --deep を使わない"
    );
}

#[test]
fn mas_team_id_is_wired_in_design_and_submit_guide() {
    let root = tauri_dir().join("../../..");
    let design = read(&root.join("docs/design.md"));
    assert!(
        design.contains("CHSRTG92SZ.jp.riku.winzip") && design.contains("<string>CHSRTG92SZ</string>"),
        "設計 §4.1 の entitlements 例に実 Team ID を書く"
    );
    assert!(
        !design.contains("$TEAM_ID.jp.riku.winzip") && !design.contains("<string>$TEAM_ID</string>"),
        "設計に $TEAM_ID プレースホルダを残さない"
    );
    assert!(
        design.contains("CHSRTG92SZ") && design.contains("docs/release/submit-guide.md"),
        "設計 §9.2 で Team ID を決め、提出ガイドを指す"
    );

    let guide = read(&root.join("docs/release/submit-guide.md"));
    for needle in [
        "CHSRTG92SZ",
        "jp.riku.winzip",
        "文字化けしないZIP",
        "Apple Distribution",
        "Mac Installer Distribution",
        "¥900",
        "日本",
        "DSA",
        "Transporter",
        "ITSAppUsesNonExemptEncryption",
        "apps/winzip-jp/scripts/appstore-pkg.sh",
        "docs/store/listing.md",
    ] {
        assert!(guide.contains(needle), "提出ガイドに「{needle}」が無い");
    }
    assert!(
        !guide.contains("BEGIN PRIVATE KEY")
            && !guide.contains("-----BEGIN")
            && !guide.contains("AuthKey_"),
        "提出ガイドに秘密鍵や API キーを書かない"
    );
}
