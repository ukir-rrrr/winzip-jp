use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zipcore::{ExtractReport, ListReport, ListedEntry, NameEncoding, Plan, Progress, WriteReport};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDto {
    pub paths: Vec<String>,
    #[serde(default)]
    pub replacements: BTreeMap<String, String>,
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
pub struct PlanDto {
    pub entries: Vec<String>,
    pub issues: Vec<zipcore::Unmappable>,
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
            issues: plan.issues,
            excluded: plan.excluded,
            dest_name_hint,
        }
    }
}

pub fn display_path(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
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
