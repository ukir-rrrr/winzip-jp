//! サンドボックス検証。画面は使わずコマンドの Result だけを見る。
//!
//! 開くパス・分割の入力・保存先はここで `Sandbox::grant` / `grant_save_dest` する。
//! CLI（`e2e_open`）は JSON を stdout に出すだけで grant しない。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::commands::{create_archive_direct, list_archive_direct};
use crate::dto::CreateDto;
use crate::sandbox::Sandbox;
use zipcore::split::volume_path;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeStep {
    pub path: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitProbe {
    pub dest: String,
    pub ok: bool,
    pub z01: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeReport {
    pub open: ProbeStep,
    pub deny: ProbeStep,
    pub split_container: SplitProbe,
    pub split_outside: Option<SplitProbe>,
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn incompressible(len: usize) -> Vec<u8> {
    let mut x = 0x9e3779b97f4a7c15u64;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(0xbf58476d1ce4e5b9).wrapping_add(1);
            (x >> 56) as u8
        })
        .collect()
}

fn unique_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "wzjp-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::create_dir_all(&path);
    path
}

fn split_once(sandbox: &mut Sandbox, dest: &Path) -> SplitProbe {
    let root = unique_dir("probe-split");
    let src = root.join("が.txt");
    if let Err(e) = fs::write(&src, incompressible(8000)) {
        return SplitProbe {
            dest: path_str(dest),
            ok: false,
            z01: false,
            error: Some(format!("分割用の入力を書けません: {e}")),
        };
    }
    let _ = sandbox.grant(&src);
    let _ = sandbox.grant_save_dest(dest);
    let err = create_archive_direct(
        sandbox,
        CreateDto {
            paths: vec![path_str(&src)],
            replacements: Default::default(),
            split_bytes: Some(512),
            ..Default::default()
        },
        &path_str(dest),
        |_| {},
        &AtomicBool::new(false),
    )
    .err();
    let z01 = volume_path(dest, 1, false);
    SplitProbe {
        dest: path_str(dest),
        ok: err.is_none() && dest.exists(),
        z01: z01.exists(),
        error: err,
    }
}

fn list_step(sandbox: &Sandbox, path: &Path) -> ProbeStep {
    match list_archive_direct(sandbox, &path_str(path), None) {
        Ok(_) => ProbeStep {
            path: path_str(path),
            ok: true,
            error: None,
        },
        Err(error) => ProbeStep {
            path: path_str(path),
            ok: false,
            error: Some(error),
        },
    }
}

/// フィクスチャをユーザー選択扱いで開き、無関係パスはコマンド Err。
/// 分割はコンテナ内（成功）と任意の外側 dest（権限エラー）を記録する。
pub fn build_report(
    sandbox: &mut Sandbox,
    open: &Path,
    deny: &Path,
    split_outside: Option<&Path>,
) -> ProbeReport {
    let _ = sandbox.grant(open);
    ProbeReport {
        open: list_step(sandbox, open),
        deny: list_step(sandbox, deny),
        split_container: {
            let dest = unique_dir("probe-split-dest").join("out.zip");
            split_once(sandbox, &dest)
        },
        split_outside: split_outside.map(|p| split_once(sandbox, p)),
    }
}

fn sandbox_write_denied(err: Option<&str>) -> bool {
    let err = err.unwrap_or("");
    if err.contains("見つかりません") {
        return false;
    }
    err.contains("権限がありません")
        || err.contains("アクセスできません")
        || err.contains("保存先へ移せませんでした")
}

/// フィクスチャ読取成功・無関係パス拒否・コンテナ内分割成功。
/// 外側 dest があるときは権限／アクセス拒否であること（親無しの NotFound は含めない）。
pub fn report_passes(report: &ProbeReport) -> bool {
    let deny_err = report.deny.error.as_deref().unwrap_or("");
    if !(report.open.ok
        && !report.deny.ok
        && deny_err.contains("アクセスできません")
        && report.split_container.ok
        && report.split_container.z01)
    {
        return false;
    }
    match &report.split_outside {
        None => true,
        Some(outside) => !outside.ok && sandbox_write_denied(outside.error.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("workspace root")
    }

    #[test]
    fn fixture_lists_and_unrelated_path_is_command_err() {
        let zip = workspace_root().join("tests/fixtures/read/cp932-typical.zip");
        let deny = workspace_root().join("README.md");
        let mut sandbox = Sandbox::new();
        let report = build_report(&mut sandbox, &zip, &deny, None);
        assert!(report.open.ok, "{:?}", report.open);
        assert!(!report.deny.ok, "{:?}", report.deny);
        let err = report.deny.error.as_deref().unwrap_or("");
        assert!(err.contains("アクセスできません"), "{err}");
        assert!(report.split_container.ok, "{:?}", report.split_container);
        assert!(report.split_container.z01, "{:?}", report.split_container);
        assert!(report.split_outside.is_none());
        assert!(report_passes(&report), "{report:?}");
    }

    #[test]
    fn split_outside_missing_parent_is_japanese_err() {
        let zip = workspace_root().join("tests/fixtures/read/cp932-typical.zip");
        let deny = workspace_root().join("README.md");
        let outside = unique_dir("probe-out").join("missing-parent").join("out.zip");
        let mut sandbox = Sandbox::new();
        let report = build_report(&mut sandbox, &zip, &deny, Some(&outside));
        let split = report.split_outside.as_ref().expect("outside");
        assert!(!split.ok, "{split:?}");
        let err = split.error.as_deref().unwrap_or("");
        assert!(err.contains("見つかりません"), "{err}");
        assert!(
            !report_passes(&report),
            "親無しの NotFound をサンドボックス拒否とみなさない: {report:?}"
        );
    }
}
