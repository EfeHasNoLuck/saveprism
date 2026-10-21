/// Returns the version of the SavePrism core crate.
#[tauri::command]
fn core_version() -> &'static str {
    saveprism_core::VERSION
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![core_version])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        eprintln!("SP-APP-001: failed to start SavePrism: {err}");
        std::process::exit(1);
    }
}
