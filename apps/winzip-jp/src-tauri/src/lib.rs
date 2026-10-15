#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = zipcore::EFS_UTF8_BIT;

    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("アプリの起動に失敗しました");
}
