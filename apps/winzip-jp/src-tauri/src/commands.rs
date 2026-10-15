//! Tauri コマンド。フロントはパスと設定だけ渡し、中身は読まない。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::Emitter;
use zipcore::{
    absolute, encode_entry_name, extract_zip, inspect_zip_headers, list_zip, normalize_lexically,
    path_eq, plan_write, stays_under, write_zip, AesMode, ExtractRequest, InputItem, ListRequest,
    NameEncoding, Progress, WriteRequest, ZipcoreError, EFS_UTF8_BIT,
};

use crate::dto::{
    path_buf, starting_progress, CreateDto, ExtractDto, ExtractReportDto, ListReportDto, PlanDto,
    ProgressDto, WriteReportDto,
};
use crate::sandbox::{container_tmp_zip, discard_tmp, publish_tmp, PublishError, Sandbox};

pub const PROGRESS_EVENT: &str = "zip://progress";
pub const DROPPED_EVENT: &str = "zip://dropped";

pub struct AppState {
    pub sandbox: Arc<Mutex<Sandbox>>,
    pub cancel: Arc<AtomicBool>,
    pub busy: Arc<AtomicBool>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sandbox: Arc::new(Mutex::new(Sandbox::new())),
            cancel: Arc::new(AtomicBool::new(false)),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }
}

struct BusyGuard(Arc<AtomicBool>);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub fn emit_progress(window: &tauri::Window, progress: &Progress) {
    let _ = window.emit(PROGRESS_EVENT, &ProgressDto::from(progress));
}

fn map_err(err: ZipcoreError) -> String {
    err.to_string()
}

fn lock_sandbox(
    sandbox: &Arc<Mutex<Sandbox>>,
) -> Result<std::sync::MutexGuard<'_, Sandbox>, String> {
    sandbox
        .lock()
        .map_err(|_| "内部状態をロックできません".to_string())
}

fn io_ja(err: &std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::NotFound => "見つかりません",
        std::io::ErrorKind::PermissionDenied => "権限がありません",
        _ => "読み書きに失敗しました",
    }
    .into()
}

fn publish_err_ja(err: PublishError) -> String {
    match err {
        PublishError::Io(e) => format!("保存先へ移せませんでした: {}", io_ja(&e)),
        PublishError::BackupKept { source, backup } => format!(
            "保存先へ移せませんでした: {}（元の保存先の退避: {}）",
            io_ja(&source),
            backup.display()
        ),
    }
}

fn resolve_inputs(sandbox: &Sandbox, paths: &[String]) -> Result<Vec<InputItem>, String> {
    let mut inputs = Vec::with_capacity(paths.len());
    for raw in paths {
        let given = path_buf(raw);
        let path = sandbox.resolve(&given)?;
        let abs = normalize_lexically(&absolute(&given));
        let meta = fs::symlink_metadata(&abs)
            .map_err(|_| format!("入力が見つかりません: {}", abs.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!("シンボリックリンクは扱えません: {}", abs.display()));
        }
        inputs.push(InputItem {
            as_dir: meta.is_dir(),
            abs_path: path,
        });
    }
    if inputs.is_empty() {
        return Err(ZipcoreError::EmptyInputs.to_string());
    }
    Ok(inputs)
}

fn planned_sources_in_scope(sandbox: &Sandbox, inputs: &[InputItem]) -> Result<(), String> {
    let placeholder = container_tmp_zip();
    let plan = plan_write(&WriteRequest {
        inputs: inputs.to_vec(),
        dest: placeholder,
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    })
    .map_err(map_err)?;
    for entry in &plan.entries {
        if !sandbox.allows(&entry.source) {
            return Err(format!("スコープ外のパスです: {}", entry.source.display()));
        }
    }
    Ok(())
}

pub fn create_archive_direct(
    sandbox: &Sandbox,
    req: &CreateDto,
    dest: &str,
    mut progress: impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<WriteReportDto, String> {
    progress(starting_progress());
    let dest = sandbox.resolve_save_file(&path_buf(dest))?;
    let inputs = resolve_inputs(sandbox, &req.paths)?;
    planned_sources_in_scope(sandbox, &inputs)?;
    if inputs.iter().any(|item| {
        path_eq(&dest, &item.abs_path) || (item.as_dir && stays_under(&dest, &item.abs_path))
    }) {
        return Err(map_err(ZipcoreError::DestOverlapsInput {
            dest: dest.clone(),
        }));
    }

    let tmp = TmpGuard(container_tmp_zip());
    let write_req = WriteRequest {
        inputs,
        dest: tmp.0.clone(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: req.replacements.clone(),
    };
    let report = write_zip(write_req, &mut progress, cancel).map_err(map_err)?;
    if inspect_zip_headers(&tmp.0)
        .map_err(map_err)?
        .iter()
        .any(|e| e.gpbf & EFS_UTF8_BIT != 0)
    {
        return Err("ファイル名の UTF-8 フラグ（EFS）が立っています".into());
    }
    publish_tmp(&tmp.0, &dest).map_err(publish_err_ja)?;
    Ok(WriteReportDto::from(report).with_dest(&dest))
}

struct TmpGuard(PathBuf);

impl Drop for TmpGuard {
    fn drop(&mut self) {
        discard_tmp(&self.0);
    }
}

pub fn list_archive_direct(
    sandbox: &Sandbox,
    path: &str,
    encoding: Option<NameEncoding>,
) -> Result<ListReportDto, String> {
    let archive = sandbox.resolve(&path_buf(path))?;
    let report = list_zip(ListRequest { archive, encoding }).map_err(map_err)?;
    Ok(ListReportDto::from(report))
}

pub fn extract_archive_direct(
    sandbox: &Sandbox,
    req: &ExtractDto,
    mut progress: impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<ExtractReportDto, String> {
    progress(starting_progress());
    let archive = sandbox.resolve(&path_buf(&req.archive))?;
    let dest = sandbox.resolve(&path_buf(&req.dest))?;
    if !dest.is_dir() {
        return Err("展開先がフォルダではありません".into());
    }
    let report = extract_zip(
        ExtractRequest {
            archive,
            dest,
            encoding: req.encoding,
            overwrite: req.overwrite,
            password: None,
        },
        &mut progress,
        cancel,
    )
    .map_err(map_err)?;
    Ok(ExtractReportDto::from(report))
}

pub fn plan_archive_direct(
    sandbox: &Sandbox,
    paths: &[String],
    dest_name_hint: Option<String>,
) -> Result<PlanDto, String> {
    let inputs = resolve_inputs(sandbox, paths)?;
    let plan = plan_write(&WriteRequest {
        inputs,
        dest: container_tmp_zip(),
        password: None,
        aes: AesMode::Aes256,
        split_bytes: None,
        replacements: BTreeMap::new(),
    })
    .map_err(map_err)?;
    Ok(PlanDto::from_plan(plan, dest_name_hint))
}

/// フロントから来たパスは grant しない。許可済みのドロップだけを返す。
pub fn accepted_dropped_paths(sandbox: &Sandbox, paths: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for raw in paths {
        if raw.contains('\0') {
            return Err("パスが不正です".into());
        }
        let abs = zipcore::absolute(&path_buf(raw));
        if sandbox.allows(&abs) {
            out.push(abs.to_string_lossy().into_owned());
        }
    }
    Ok(out)
}

fn extend_fs_scope(app: &tauri::AppHandle, paths: &[PathBuf]) {
    use tauri_plugin_fs::FsExt;
    let scope = app.fs_scope();
    for path in paths {
        if path.is_dir() {
            let _ = scope.allow_directory(path, true);
        } else {
            let _ = scope.allow_file(path);
        }
    }
}

enum GrantKind {
    Selected,
    SaveDest,
}

fn admit(
    app: &tauri::AppHandle,
    state: &AppState,
    paths: &[PathBuf],
    kind: GrantKind,
) -> Result<Vec<PathBuf>, String> {
    let mut sandbox = lock_sandbox(&state.sandbox)?;
    let mut kept = Vec::new();
    for path in paths {
        let ok = match kind {
            GrantKind::Selected => sandbox.grant(path),
            GrantKind::SaveDest => sandbox.grant_save_dest(path),
        };
        if ok {
            kept.push(path.clone());
        }
    }
    drop(sandbox);
    extend_fs_scope(app, &kept);
    Ok(kept)
}

fn file_path_to_buf(path: tauri_plugin_dialog::FilePath) -> Result<PathBuf, String> {
    path.into_path()
        .map_err(|_| "選んだ場所をパスにできません".to_string())
}

pub fn names_are_ready(names: &[String]) -> bool {
    names.iter().all(|name| encode_entry_name(name).issues.is_empty())
}

#[tauri::command]
pub fn check_names(names: Vec<String>) -> bool {
    names_are_ready(&names)
}

#[tauri::command]
pub fn plan_archive(
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
    dest_name_hint: Option<String>,
) -> Result<PlanDto, String> {
    let sandbox = lock_sandbox(&state.sandbox)?;
    plan_archive_direct(&sandbox, &paths, dest_name_hint)
}

#[tauri::command]
pub async fn create_archive(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    req: CreateDto,
    dest: String,
) -> Result<WriteReportDto, String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("別の処理が実行中です".into());
    }
    let _busy = BusyGuard(Arc::clone(&state.busy));
    state.cancel.store(false, Ordering::SeqCst);
    emit_progress(&window, &starting_progress());
    let sandbox = Arc::clone(&state.sandbox);
    let cancel = Arc::clone(&state.cancel);
    let window = window.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = lock_sandbox(&sandbox)?.snapshot_paths();
        create_archive_direct(
            &snapshot,
            &req,
            &dest,
            |p| emit_progress(&window, &p),
            &cancel,
        )
    })
    .await
    .map_err(|_| "作成処理を開始できませんでした".to_string())?
}

#[tauri::command]
pub fn list_archive(
    state: tauri::State<'_, AppState>,
    path: String,
    encoding: Option<NameEncoding>,
) -> Result<ListReportDto, String> {
    let sandbox = lock_sandbox(&state.sandbox)?;
    list_archive_direct(&sandbox, &path, encoding)
}

#[tauri::command]
pub async fn extract_archive(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    req: ExtractDto,
) -> Result<ExtractReportDto, String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("別の処理が実行中です".into());
    }
    let _busy = BusyGuard(Arc::clone(&state.busy));
    state.cancel.store(false, Ordering::SeqCst);
    emit_progress(&window, &starting_progress());
    let sandbox = Arc::clone(&state.sandbox);
    let cancel = Arc::clone(&state.cancel);
    let window = window.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = lock_sandbox(&sandbox)?.snapshot_paths();
        extract_archive_direct(&snapshot, &req, |p| emit_progress(&window, &p), &cancel)
    })
    .await
    .map_err(|_| "解凍処理を開始できませんでした".to_string())?
}

#[tauri::command]
pub fn cancel_job(state: tauri::State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn pick_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_title("ファイルを選択")
        .blocking_pick_files();
    let Some(files) = picked else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    for file in files {
        paths.push(file_path_to_buf(file)?);
    }
    let kept = admit(&app, &state, &paths, GrantKind::Selected)?;
    Ok(kept
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

#[tauri::command]
pub async fn pick_save_zip(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    default_name: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let name = if default_name.trim().is_empty() {
        "archive.zip".to_string()
    } else if default_name.ends_with(".zip") {
        default_name
    } else {
        format!("{default_name}.zip")
    };
    let picked = app
        .dialog()
        .file()
        .set_title("ZIP の保存先")
        .set_file_name(&name)
        .add_filter("ZIP", &["zip"])
        .blocking_save_file();
    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file_path_to_buf(file)?;
    let kept = admit(&app, &state, &[path], GrantKind::SaveDest)?;
    Ok(kept.first().map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn pick_extract_dir(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_title("展開先フォルダ")
        .blocking_pick_folder();
    let Some(folder) = picked else {
        return Ok(None);
    };
    let path = file_path_to_buf(folder)?;
    let kept = admit(&app, &state, &[path], GrantKind::Selected)?;
    Ok(kept.first().map(|p| p.to_string_lossy().into_owned()))
}

pub fn grant_dropped(state: &AppState, app: &tauri::AppHandle, paths: &[PathBuf]) -> Vec<String> {
    match admit(app, state, paths, GrantKind::Selected) {
        Ok(kept) => kept
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_ja_is_kind_only() {
        let err = std::io::Error::new(
            std::io::ErrorKind::StorageFull,
            "元の保存先の退避: /tmp/x.zip",
        );
        assert_eq!(io_ja(&err), "読み書きに失敗しました");
    }

    #[test]
    fn publish_err_ja_includes_backup_path() {
        let path = "/tmp/winzip-jp-keep.zip";
        let err = PublishError::BackupKept {
            source: std::io::Error::new(std::io::ErrorKind::StorageFull, "ignored"),
            backup: PathBuf::from(path),
        };
        let text = publish_err_ja(err);
        assert!(text.contains("読み書きに失敗しました"));
        assert!(text.contains(path));
    }
}
