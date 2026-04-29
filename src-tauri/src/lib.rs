//! Backend library entrypoint for the Kindle desktop application.
//!
//! At this stage the library exposes the device, TOC repair and uploader
//! modules. The rest of the domain modules will be added incrementally in later
//! steps.

pub mod converter;
pub mod desktop;
pub mod device;
pub mod library;
pub mod toc;
pub mod uploader;

/// Boot the Tauri runtime for the desktop shell.
///
/// The runtime wires Tauri commands and background services into the frontend
/// bridge without forcing the UI shell to know about backend module details.
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let desktop_state = desktop::KindleDesktopState::default();
    let setup_state = desktop_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(desktop_state)
        .setup(move |app| {
            desktop::setup_desktop_runtime(app.handle().clone(), setup_state.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            desktop::get_app_state,
            desktop::refresh_devices,
            desktop::queue_upload_files,
            desktop::start_upload,
            desktop::list_kindle_books,
            desktop::delete_kindle_book,
            desktop::rename_kindle_book
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Kindle Relay");
}
