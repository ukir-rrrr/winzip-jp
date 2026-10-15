//! 作成 / 解凍で共有する進捗とパスワード。

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::ZipcoreError;

/// パスワード。Debug に中身を出さない。破棄時にゼロ化する。
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn expose_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(***)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgressPhase {
    Starting,
    Writing,
    Finishing,
    Done,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Progress {
    pub phase: ProgressPhase,
    pub current_entry: Option<String>,
    pub entries_done: u32,
    pub entries_total: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroize;

    #[test]
    fn secret_string_zeroizes() {
        let mut s = SecretString::new("hunter2");
        s.zeroize();
        assert!(s.expose().is_empty() || s.expose().bytes().all(|b| b == 0));
    }
}

pub fn check_cancel(cancel: &AtomicBool) -> Result<(), ZipcoreError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ZipcoreError::Cancelled)
    } else {
        Ok(())
    }
}
