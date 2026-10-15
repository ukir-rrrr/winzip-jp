//! `--e2e-open` は debug + `feature = "e2e"` のときだけコンパイルする。
//! App Store / release には引数名も残さない。

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::commands::{grant_dropped, AppState};
use crate::probe::build_report;
use crate::sandbox::Sandbox;

const FLAG_OPEN: &str = "--e2e-open";
const FLAG_DENY: &str = "--e2e-deny";
const FLAG_REPORT: &str = "--e2e-report";
const FLAG_SPLIT_OUT: &str = "--e2e-split-dest";

pub struct E2eArgs {
    pub open: PathBuf,
    pub deny: Option<PathBuf>,
    pub report: Option<PathBuf>,
    pub split_outside: Option<PathBuf>,
}

static STARTUP: OnceLock<E2eArgs> = OnceLock::new();

pub fn parse() -> Option<E2eArgs> {
    let mut args = std::env::args().skip(1);
    let mut open = None;
    let mut deny = None;
    let mut report = None;
    let mut split_outside = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            FLAG_OPEN => open = args.next().map(PathBuf::from),
            FLAG_DENY => deny = args.next().map(PathBuf::from),
            FLAG_REPORT => report = args.next().map(PathBuf::from),
            FLAG_SPLIT_OUT => split_outside = args.next().map(PathBuf::from),
            _ => {}
        }
    }
    Some(E2eArgs {
        open: open?,
        deny,
        report,
        split_outside,
    })
}

/// `--e2e-report` があるときはウィンドウを出さず JSON を書いて終了する。
/// 戻り値の `Some` はプロセス終了コード。
pub fn run_probe_if_requested(args: &E2eArgs) -> Option<i32> {
    let report_path = args.report.as_ref()?;
    let deny = args.deny.clone().unwrap_or_else(default_deny_path);
    let mut sandbox = Sandbox::new();
    let report = build_report(
        &mut sandbox,
        &args.open,
        &deny,
        args.split_outside.as_deref(),
    );
    let json = serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into());
    if let Some(parent) = report_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let write_ok = fs::write(report_path, json).is_ok();
    let pass = report.open.ok && !report.deny.ok && write_ok;
    Some(if pass { 0 } else { 2 })
}

pub fn stash_for_ui(args: E2eArgs) {
    let _ = STARTUP.set(args);
}

pub fn apply_startup_grant(state: &AppState, app: &tauri::AppHandle) {
    let Some(args) = STARTUP.get() else {
        return;
    };
    let _ = grant_dropped(state, app, &[args.open.clone()]);
}

fn default_deny_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("Library/Preferences/com.apple.finder.plist")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_names_are_stable() {
        assert_eq!(FLAG_OPEN, "--e2e-open");
        assert_eq!(FLAG_DENY, "--e2e-deny");
        assert_eq!(FLAG_REPORT, "--e2e-report");
    }
}
