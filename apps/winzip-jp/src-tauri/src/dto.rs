use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zipcore::{ExtractReport, ListReport, ListedEntry, NameEncoding, Plan, Progress, WriteReport};

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDto {
    pub paths: Vec<String>,
    #[serde(default)]
    pub replacements: BTreeMap<String, String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub split_bytes: Option<u64>,
}

impl std::fmt::Debug for CreateDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateDto")
            .field("paths", &self.paths)
            .field("replacements", &self.replacements)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("split_bytes", &self.split_bytes)
            .finish()
    }
}

impl CreateDto {
    pub fn take_password(&mut self) -> Result<Option<zipcore::SecretString>, String> {
        match self.password.take() {
            None => Ok(None),
            Some(s) if s.is_empty() => Err(zipcore::ZipcoreError::EmptyPassword.to_string()),
            Some(s) => Ok(Some(zipcore::SecretString::new(s))),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractDto {
    pub archive: String,
    pub dest: String,
    pub encoding: Option<NameEncoding>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteReportDto {
    pub dest: String,
    pub entries_written: u32,
    pub bytes_written: u64,
    pub excluded: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractReportDto {
    pub dest: String,
    pub entries_written: u32,
    pub bytes_written: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedEntryDto {
    pub name_hex: String,
    pub display_name: String,
    pub is_dir: bool,
    pub uncompressed_size: u64,
    pub gpbf: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListReportDto {
    pub entries: Vec<ListedEntryDto>,
    pub encoding: NameEncoding,
    pub confidence: f32,
    pub ambiguous: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmappableDto {
    pub relative_path: String,
    pub original_char: String,
    pub suggested: String,
    pub kind: zipcore::UnmappableKind,
}

impl From<zipcore::Unmappable> for UnmappableDto {
    fn from(value: zipcore::Unmappable) -> Self {
        Self {
            relative_path: value.relative_path,
            original_char: value.original_char,
            suggested: value.suggested,
            kind: value.kind,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDto {
    pub entries: Vec<String>,
    pub issues: Vec<UnmappableDto>,
    pub excluded: Vec<String>,
    pub dest_name_hint: Option<String>,
}

pub fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

impl From<WriteReport> for WriteReportDto {
    fn from(value: WriteReport) -> Self {
        Self {
            dest: display_path(&value.dest),
            entries_written: value.entries_written,
            bytes_written: value.bytes_written,
            excluded: value.excluded,
        }
    }
}

impl WriteReportDto {
    pub fn with_dest(mut self, dest: &std::path::Path) -> Self {
        self.dest = display_path(dest);
        self
    }
}

impl From<ExtractReport> for ExtractReportDto {
    fn from(value: ExtractReport) -> Self {
        Self {
            dest: display_path(&value.dest),
            entries_written: value.entries_written,
            bytes_written: value.bytes_written,
        }
    }
}

impl From<ListedEntry> for ListedEntryDto {
    fn from(value: ListedEntry) -> Self {
        Self {
            name_hex: hex_bytes(&value.name_raw),
            display_name: value.display_name,
            is_dir: value.is_dir,
            uncompressed_size: value.uncompressed_size,
            gpbf: value.gpbf,
        }
    }
}

impl From<ListReport> for ListReportDto {
    fn from(value: ListReport) -> Self {
        Self {
            entries: value
                .entries
                .into_iter()
                .map(ListedEntryDto::from)
                .collect(),
            encoding: value.encoding,
            confidence: value.detection.confidence,
            ambiguous: value.detection.ambiguous,
        }
    }
}

impl PlanDto {
    pub fn from_plan(plan: Plan, dest_name_hint: Option<String>) -> Self {
        Self {
            entries: plan.entries.into_iter().map(|e| e.relative_path).collect(),
            issues: plan.issues.into_iter().map(UnmappableDto::from).collect(),
            excluded: plan.excluded,
            dest_name_hint,
        }
    }
}

pub fn display_path(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressDto {
    pub phase: zipcore::ProgressPhase,
    pub current_entry: Option<String>,
    pub entries_done: u32,
    pub entries_total: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl From<&Progress> for ProgressDto {
    fn from(value: &Progress) -> Self {
        Self {
            phase: value.phase,
            current_entry: value.current_entry.clone(),
            entries_done: value.entries_done,
            entries_total: value.entries_total,
            bytes_done: value.bytes_done,
            bytes_total: value.bytes_total,
        }
    }
}

pub fn starting_progress() -> Progress {
    Progress {
        phase: zipcore::ProgressPhase::Starting,
        current_entry: None,
        entries_done: 0,
        entries_total: 0,
        bytes_done: 0,
        bytes_total: 0,
    }
}

pub fn path_buf(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_dto_debug_redacts_password() {
        let dto = CreateDto {
            paths: vec!["/tmp/a".into()],
            replacements: Default::default(),
            password: Some("hunter2".into()),
            split_bytes: None,
        };
        let text = format!("{dto:?}");
        assert!(text.contains("***"));
        assert!(!text.contains("hunter2"));
    }

    #[test]
    fn unmappable_dto_uses_camel_case() {
        let dto = UnmappableDto {
            relative_path: "髙.png".into(),
            original_char: "髙".into(),
            suggested: "高".into(),
            kind: zipcore::UnmappableKind::CompatIdeograph,
        };
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(value["relativePath"], "髙.png");
        assert_eq!(value["originalChar"], "髙");
        assert_eq!(value["suggested"], "高");
        assert_eq!(value["kind"], "compatIdeograph");
        assert!(value.get("relative_path").is_none());
    }
}
