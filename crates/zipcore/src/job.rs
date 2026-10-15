//! 作成 / 解凍で共有する進捗とパスワード。

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::error::ZipcoreError;

/// パスワード。Debug に中身を出さない。
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
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

pub fn check_cancel(cancel: &AtomicBool) -> Result<(), ZipcoreError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ZipcoreError::Cancelled)
    } else {
        Ok(())
    }
}
