//! `--e2e-open` は debug + `feature = "e2e"` のときだけコンパイルする。
//! App Store / release には引数名も残さない。
//!
//! フラグがあればヘッドレスで JSON を stdout に書く（サンドボックスは
//! ワークスペースへファイルを作れない。親プロセスがリダイレクトする）。
//! S1 はフラグなしの通常起動。

use std::io::{self, Write};
use std::path::PathBuf;

use crate::probe::{build_report, report_passes};
use crate::sandbox::Sandbox;

const FLAG_OPEN: &str = "--e2e-open";
const FLAG_DENY: &str = "--e2e-deny";
const FLAG_SPLIT_OUT: &str = "--e2e-split-dest";

pub struct E2eArgs {
    pub open: PathBuf,
    pub deny: PathBuf,
    pub split_outside: Option<PathBuf>,
}

struct RawFlags {
    open: Option<PathBuf>,
    deny: Option<PathBuf>,
    split_outside: Option<PathBuf>,
}

impl RawFlags {
    fn any(&self) -> bool {
        self.open.is_some() || self.deny.is_some() || self.split_outside.is_some()
    }
}

fn parse_raw() -> RawFlags {
    let mut args = std::env::args().skip(1);
    let mut open = None;
    let mut deny = None;
    let mut split_outside = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            FLAG_OPEN => open = args.next().map(PathBuf::from),
            FLAG_DENY => deny = args.next().map(PathBuf::from),
            FLAG_SPLIT_OUT => split_outside = args.next().map(PathBuf::from),
            _ => {}
        }
    }
    RawFlags {
        open,
        deny,
        split_outside,
    }
}

/// e2e フラグが無ければ `None`（通常起動）。あれば終了コード。
pub fn maybe_run() -> Option<i32> {
    let raw = parse_raw();
    if !raw.any() {
        return None;
    }
    let (Some(open), Some(deny)) = (raw.open, raw.deny) else {
        eprintln!("--e2e-open と --e2e-deny が必要です");
        return Some(2);
    };
    Some(run_probe(&E2eArgs {
        open,
        deny,
        split_outside: raw.split_outside,
    }))
}

fn run_probe(args: &E2eArgs) -> i32 {
    let mut sandbox = Sandbox::new();
    let report = build_report(
        &mut sandbox,
        &args.open,
        &args.deny,
        args.split_outside.as_deref(),
    );
    let json = serde_json::to_vec_pretty(&report).unwrap_or_else(|_| b"{}".to_vec());
    let mut out = io::stdout();
    let _ = out.write_all(&json);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
    if report_passes(&report) {
        0
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_names_match_design() {
        assert_eq!(FLAG_OPEN, "--e2e-open");
        assert_eq!(FLAG_DENY, "--e2e-deny");
        assert_eq!(FLAG_SPLIT_OUT, "--e2e-split-dest");
    }
}
