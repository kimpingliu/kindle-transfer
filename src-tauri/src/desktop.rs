//! Tauri-facing desktop state and command bridge.
//!
//! This module adapts the lower-level detector and uploader modules into a
//! frontend-friendly state model:
//!
//! - Device snapshots are normalized into the React view model shape.
//! - Upload queue items are tracked across commands and progress callbacks.
//! - Transfer history is accumulated in memory and emitted back to the UI.
//! - USB watcher updates and upload progress are pushed through a single
//!   `kindle://state` event channel so the frontend can stay declarative.

use crate::converter::{
    ConversionRequest, ConversionWorkspace, ConverterError, EbookConversionService, KindleFormat,
};
use crate::device::usb_detector::{KindleDevice, UsbDetector, UsbWatchEvent};
use crate::library::{
    delete_kindle_book_by_id, rename_kindle_book_by_id, scan_kindle_books, DeleteKindleBookResult,
    KindleLibraryBook, KindleLibraryError,
};
use crate::uploader::{
    ProgressCallback, UploadItem, UploadItemResult, UploadItemStatus, UploadManager,
    UploadProgressEvent, UploadRequest, UploadResult, UploadStage, UploadTarget, UsbUploadTarget,
};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use thiserror::Error;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{error, warn};
use uuid::Uuid;

const DEFAULT_STORAGE_TOTAL_GB: f64 = 8.0;
const HISTORY_LIMIT: usize = 200;
const STATE_EVENT_NAME: &str = "kindle://state";

/// Shared desktop application state.
#[derive(Clone, Default)]
pub struct KindleDesktopState {
    store: Arc<RwLock<DesktopStore>>,
    usb_detector: UsbDetector,
    upload_manager: UploadManager,
    upload_active: Arc<AtomicBool>,
}

impl KindleDesktopState {
    /// Build the serializable state snapshot consumed by the React frontend.
    pub async fn snapshot(&self) -> FrontendStateSnapshot {
        self.store.read().await.snapshot()
    }
}

#[derive(Debug, Default)]
struct DesktopStore {
    usb_devices: BTreeMap<String, KindleDeviceView>,
    upload_queue: Vec<UploadQueueItemView>,
    history: Vec<HistoryRecordView>,
}

impl DesktopStore {
    fn snapshot(&self) -> FrontendStateSnapshot {
        let mut devices = self.usb_devices.values().cloned().collect::<Vec<_>>();

        devices.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));

        FrontendStateSnapshot {
            devices,
            upload_queue: self.upload_queue.clone(),
            history: self.history.clone(),
        }
    }

    fn replace_usb_devices(&mut self, devices: Vec<KindleDeviceView>) {
        self.usb_devices = devices
            .into_iter()
            .map(|device| (device.id.clone(), device))
            .collect();
    }

    fn find_device(&self, device_id: &str) -> Option<KindleDeviceView> {
        self.usb_devices.get(device_id).cloned()
    }

    fn queue_item_mut(&mut self, queue_id: &str) -> Option<&mut UploadQueueItemView> {
        self.upload_queue
            .iter_mut()
            .find(|item| item.id == queue_id)
    }

    fn upsert_queue_item(&mut self, mut item: UploadQueueItemView) {
        if let Some(existing) = self
            .upload_queue
            .iter_mut()
            .find(|existing| same_queue_source(existing, &item))
        {
            if is_active_queue_stage(&existing.stage) {
                return;
            }

            item.id = existing.id.clone();
            *existing = item;
            return;
        }

        self.upload_queue.push(item);
    }

    fn prepend_history(&mut self, record: HistoryRecordView) {
        self.history.insert(0, record);
        if self.history.len() > HISTORY_LIMIT {
            self.history.truncate(HISTORY_LIMIT);
        }
    }
}

/// Frontend state snapshot returned by commands and events.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendStateSnapshot {
    pub devices: Vec<KindleDeviceView>,
    pub upload_queue: Vec<UploadQueueItemView>,
    pub history: Vec<HistoryRecordView>,
}

/// React-facing device view model.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindleDeviceView {
    pub id: String,
    pub name: String,
    pub model: String,
    pub firmware: String,
    pub connection: String,
    pub status: String,
    pub upload_available: bool,
    pub battery_level: u8,
    pub storage_used_gb: f64,
    pub storage_total_gb: f64,
    pub ip_address: Option<String>,
    pub mount_path: Option<String>,
    pub supported_formats: Vec<String>,
    pub last_seen_label: String,
    pub endpoint: Option<String>,
}

/// React-facing queue item view model.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadQueueItemView {
    pub id: String,
    pub title: String,
    pub author: String,
    pub source_format: String,
    pub target_format: String,
    pub size_mb: f64,
    pub convert_progress: u8,
    pub upload_progress: u8,
    pub stage: String,
    pub destination_path: Option<String>,
    #[serde(skip_serializing)]
    source_path: String,
    #[serde(skip_serializing)]
    device_id: String,
    #[serde(skip_serializing)]
    size_bytes: u64,
}

/// React-facing transfer history record.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecordView {
    pub id: String,
    pub title: String,
    pub device_name: String,
    pub connection: String,
    pub output_format: String,
    pub transferred_at: String,
    pub duration_label: String,
    pub size_label: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueUploadRequest {
    pub device_id: String,
    pub file_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartUploadRequest {
    pub device_id: String,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListKindleBooksRequest {
    pub device_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteKindleBookRequest {
    pub device_id: String,
    pub book_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameKindleBookRequest {
    pub device_id: String,
    pub book_id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
struct QueueBinding {
    queue_id: String,
    title: String,
    device_name: String,
    connection: String,
    output_format: String,
    source_path: PathBuf,
    size_bytes: u64,
}

#[derive(Debug)]
struct PreparedUploadBatch {
    bindings: Vec<QueueBinding>,
    items: Vec<UploadItem>,
    _workspace: ConversionWorkspace,
}

#[derive(Debug, Error)]
enum DesktopBridgeError {
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    #[error("upload queue is empty for device {0}")]
    EmptyQueue(String),
    #[error("an upload is already in progress")]
    UploadAlreadyRunning,
    #[error("ebook conversion failed: {0}")]
    Conversion(#[from] ConverterError),
    #[error("Kindle library operation failed: {0}")]
    Library(#[from] KindleLibraryError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("USB detection failed: {0}")]
    UsbDetection(#[from] crate::device::usb_detector::UsbDetectorError),
}

/// Start the background services required by the desktop shell.
pub fn setup_desktop_runtime(app: AppHandle, state: KindleDesktopState) {
    let app_handle = app.clone();
    let state_for_watch = state.clone();

    match state.usb_detector.start_watch() {
        Ok(watch_handle) => {
            std::thread::spawn(move || {
                let watch_handle = watch_handle;
                while let Ok(event) = watch_handle.recv() {
                    match event {
                        UsbWatchEvent::Snapshot(devices) => {
                            let app = app_handle.clone();
                            let state = state_for_watch.clone();
                            tauri::async_runtime::spawn(async move {
                                apply_usb_snapshot(&state, devices).await;
                                emit_state_snapshot(&app, &state).await;
                            });
                        }
                        UsbWatchEvent::Error(message) => {
                            warn!("USB watcher reported a non-fatal error: {message}");
                        }
                        UsbWatchEvent::Connected(_)
                        | UsbWatchEvent::Updated(_)
                        | UsbWatchEvent::Disconnected(_) => {}
                    }
                }
            });
        }
        Err(error) => {
            warn!("failed to start USB watcher: {error}");
        }
    }

    let app_handle = app.clone();
    let bootstrap_state = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = refresh_all_devices(&bootstrap_state).await {
            warn!("initial device refresh failed: {error}");
        }
        emit_state_snapshot(&app_handle, &bootstrap_state).await;
    });
}

/// Return the latest app state known by the backend.
#[tauri::command]
pub async fn get_app_state(
    state: State<'_, KindleDesktopState>,
) -> Result<FrontendStateSnapshot, String> {
    Ok(state.snapshot().await)
}

/// Force a USB device refresh.
#[tauri::command]
pub async fn refresh_devices(
    app: AppHandle,
    state: State<'_, KindleDesktopState>,
) -> Result<FrontendStateSnapshot, String> {
    refresh_all_devices(state.inner())
        .await
        .map_err(|error| error.to_string())?;
    emit_state_snapshot(&app, state.inner()).await;
    Ok(state.snapshot().await)
}

/// Queue local files for a future upload run.
#[tauri::command]
pub async fn queue_upload_files(
    app: AppHandle,
    state: State<'_, KindleDesktopState>,
    request: QueueUploadRequest,
) -> Result<FrontendStateSnapshot, String> {
    let device = {
        let store = state.store.read().await;
        store
            .find_device(&request.device_id)
            .ok_or_else(|| DesktopBridgeError::DeviceNotFound(request.device_id.clone()))
            .map_err(|error| error.to_string())?
    };

    let mut queued_items = Vec::new();
    for file_path in request.file_paths {
        let source_path = PathBuf::from(file_path);
        let metadata = fs::metadata(&source_path)
            .await
            .map_err(|error| DesktopBridgeError::from(error).to_string())?;

        if !metadata.is_file() {
            continue;
        }

        let file_name = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("book")
            .to_string();
        let source_format = file_extension_uppercase(&source_path);
        let title = source_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(&file_name)
            .to_string();

        queued_items.push(UploadQueueItemView {
            id: Uuid::new_v4().to_string(),
            title,
            author: "本地导入".to_string(),
            source_format,
            target_format: preferred_target_format(&device),
            size_mb: round_size_mb(metadata.len()),
            convert_progress: 0,
            upload_progress: 0,
            stage: "queued".to_string(),
            destination_path: None,
            source_path: source_path.to_string_lossy().into_owned(),
            device_id: request.device_id.clone(),
            size_bytes: metadata.len(),
        });
    }

    {
        let mut store = state.store.write().await;
        for item in queued_items {
            store.upsert_queue_item(item);
        }
    }

    emit_state_snapshot(&app, state.inner()).await;
    Ok(state.snapshot().await)
}

/// Start uploading all queued files for the selected device.
#[tauri::command]
pub async fn start_upload(
    app: AppHandle,
    state: State<'_, KindleDesktopState>,
    request: StartUploadRequest,
) -> Result<FrontendStateSnapshot, String> {
    let desktop_state = state.inner().clone();

    if desktop_state.upload_active.swap(true, Ordering::SeqCst) {
        return Err(DesktopBridgeError::UploadAlreadyRunning.to_string());
    }

    let device = {
        let store = desktop_state.store.read().await;
        store
            .find_device(&request.device_id)
            .ok_or_else(|| DesktopBridgeError::DeviceNotFound(request.device_id.clone()))
            .map_err(|error| {
                desktop_state.upload_active.store(false, Ordering::SeqCst);
                error.to_string()
            })?
    };

    let bindings = {
        let mut store = desktop_state.store.write().await;
        let mut bindings = Vec::new();

        for item in store
            .upload_queue
            .iter_mut()
            .filter(|item| item.device_id == request.device_id && item.stage == "queued")
        {
            item.stage = "converting".to_string();
            item.convert_progress = item.convert_progress.max(8);

            bindings.push(QueueBinding {
                queue_id: item.id.clone(),
                title: item.title.clone(),
                device_name: device.name.clone(),
                connection: device.connection.clone(),
                output_format: item.target_format.clone(),
                source_path: PathBuf::from(item.source_path.clone()),
                size_bytes: item.size_bytes,
            });
        }

        bindings
    };

    if bindings.is_empty() {
        desktop_state.upload_active.store(false, Ordering::SeqCst);
        return Err(DesktopBridgeError::EmptyQueue(request.device_id).to_string());
    }

    let snapshot_state = desktop_state.clone();
    let pending_bindings = bindings.clone();
    let overwrite = request.overwrite.unwrap_or(true);
    tauri::async_runtime::spawn(async move {
        emit_state_snapshot(&app, &desktop_state).await;

        let prepared_batch = match prepare_upload_batch(&desktop_state, &app, bindings).await {
            Ok(prepared_batch) => prepared_batch,
            Err(error) => {
                fail_upload_batch(&desktop_state, &app, &pending_bindings, &error.to_string())
                    .await;
                desktop_state.upload_active.store(false, Ordering::SeqCst);
                emit_state_snapshot(&app, &desktop_state).await;
                return;
            }
        };

        if prepared_batch.items.is_empty() {
            desktop_state.upload_active.store(false, Ordering::SeqCst);
            emit_state_snapshot(&app, &desktop_state).await;
            return;
        }

        let upload_request = UploadRequest {
            target: build_upload_target(&device),
            items: prepared_batch.items.clone(),
            overwrite,
        };
        let queue_bindings = Arc::new(prepared_batch.bindings.clone());
        let progress_callback =
            build_progress_callback(desktop_state.clone(), app.clone(), queue_bindings.clone());
        let upload_manager = desktop_state.upload_manager.clone();

        let result = upload_manager
            .upload(upload_request, Some(progress_callback))
            .await;

        match result {
            Ok(result) => {
                finalize_upload_result(&desktop_state, &app, queue_bindings.as_ref(), &result)
                    .await;
            }
            Err(error) => {
                fail_upload_batch(
                    &desktop_state,
                    &app,
                    queue_bindings.as_ref(),
                    &error.to_string(),
                )
                .await;
            }
        }

        desktop_state.upload_active.store(false, Ordering::SeqCst);
        emit_state_snapshot(&app, &desktop_state).await;
    });

    Ok(snapshot_state.snapshot().await)
}

/// List books that currently exist in the selected Kindle `documents/` folder.
#[tauri::command]
pub async fn list_kindle_books(
    state: State<'_, KindleDesktopState>,
    request: ListKindleBooksRequest,
) -> Result<Vec<KindleLibraryBook>, String> {
    let mount_path = selected_device_mount_path(state.inner(), &request.device_id)
        .await
        .map_err(|error| error.to_string())?;

    scan_kindle_books(mount_path)
        .await
        .map_err(|error| error.to_string())
}

/// Delete a book that was returned by `list_kindle_books`.
#[tauri::command]
pub async fn delete_kindle_book(
    state: State<'_, KindleDesktopState>,
    request: DeleteKindleBookRequest,
) -> Result<DeleteKindleBookResult, String> {
    let mount_path = selected_device_mount_path(state.inner(), &request.device_id)
        .await
        .map_err(|error| error.to_string())?;

    delete_kindle_book_by_id(mount_path, request.book_id)
        .await
        .map_err(|error| error.to_string())
}

/// Rename a book that was returned by `list_kindle_books`.
#[tauri::command]
pub async fn rename_kindle_book(
    state: State<'_, KindleDesktopState>,
    request: RenameKindleBookRequest,
) -> Result<KindleLibraryBook, String> {
    let mount_path = selected_device_mount_path(state.inner(), &request.device_id)
        .await
        .map_err(|error| error.to_string())?;

    rename_kindle_book_by_id(mount_path, request.book_id, request.title)
        .await
        .map_err(|error| error.to_string())
}

async fn selected_device_mount_path(
    state: &KindleDesktopState,
    device_id: &str,
) -> Result<PathBuf, DesktopBridgeError> {
    let device = {
        let store = state.store.read().await;
        store
            .find_device(device_id)
            .ok_or_else(|| DesktopBridgeError::DeviceNotFound(device_id.to_string()))?
    };

    Ok(PathBuf::from(device.mount_path.unwrap_or_default()))
}

async fn prepare_upload_batch(
    state: &KindleDesktopState,
    app: &AppHandle,
    bindings: Vec<QueueBinding>,
) -> Result<PreparedUploadBatch, DesktopBridgeError> {
    let workspace = ConversionWorkspace::new()?;
    let converter = EbookConversionService::default();
    let mut prepared_bindings = Vec::new();
    let mut prepared_items = Vec::new();
    let mut conversion_failures = Vec::new();

    for binding in bindings {
        update_queue_conversion_progress(state, app, &binding.queue_id, 34).await;

        let request = ConversionRequest::new(binding.source_path.clone(), KindleFormat::Azw3);
        match converter.prepare_for_kindle(&request, &workspace).await {
            Ok(prepared_book) => {
                update_queue_conversion_ready(
                    state,
                    app,
                    &binding.queue_id,
                    &prepared_book.output_format,
                    prepared_book.size_bytes,
                )
                .await;

                let mut prepared_binding = binding.clone();
                prepared_binding.output_format = prepared_book.output_format.clone();
                prepared_binding.size_bytes = prepared_book.size_bytes;

                prepared_bindings.push(prepared_binding);
                prepared_items.push(UploadItem {
                    source_path: prepared_book.prepared_path,
                    file_name: Some(prepared_book.destination_file_name),
                    mime_type: None,
                });
            }
            Err(error) => {
                conversion_failures.push((binding, error.to_string()));
            }
        }
    }

    if !conversion_failures.is_empty() {
        fail_conversion_bindings(state, app, &conversion_failures).await;
    }

    Ok(PreparedUploadBatch {
        bindings: prepared_bindings,
        items: prepared_items,
        _workspace: workspace,
    })
}

async fn update_queue_conversion_progress(
    state: &KindleDesktopState,
    app: &AppHandle,
    queue_id: &str,
    progress: u8,
) {
    let mut store = state.store.write().await;
    if let Some(queue_item) = store.queue_item_mut(queue_id) {
        if is_terminal_queue_stage(&queue_item.stage) {
            return;
        }

        queue_item.stage = "converting".to_string();
        queue_item.convert_progress = queue_item.convert_progress.max(progress);
        queue_item.upload_progress = 0;
        queue_item.destination_path = None;
    }

    drop(store);
    emit_state_snapshot(app, state).await;
}

async fn update_queue_conversion_ready(
    state: &KindleDesktopState,
    app: &AppHandle,
    queue_id: &str,
    output_format: &str,
    size_bytes: u64,
) {
    let mut store = state.store.write().await;
    if let Some(queue_item) = store.queue_item_mut(queue_id) {
        if is_terminal_queue_stage(&queue_item.stage) {
            return;
        }

        queue_item.stage = "converting".to_string();
        queue_item.target_format = output_format.to_string();
        queue_item.convert_progress = 100;
        queue_item.upload_progress = 0;
        queue_item.size_bytes = size_bytes;
        queue_item.size_mb = round_size_mb(size_bytes);
        queue_item.destination_path = None;
    }

    drop(store);
    emit_state_snapshot(app, state).await;
}

async fn fail_conversion_bindings(
    state: &KindleDesktopState,
    app: &AppHandle,
    failures: &[(QueueBinding, String)],
) {
    let mut store = state.store.write().await;

    for (binding, error_message) in failures {
        if let Some(queue_item) = store.queue_item_mut(&binding.queue_id) {
            queue_item.stage = "failed".to_string();
            queue_item.convert_progress = 100;
            queue_item.upload_progress = 0;
            queue_item.destination_path = None;
        }

        store.prepend_history(HistoryRecordView {
            id: Uuid::new_v4().to_string(),
            title: binding.title.clone(),
            device_name: binding.device_name.clone(),
            connection: binding.connection.clone(),
            output_format: binding.output_format.clone(),
            transferred_at: format_history_timestamp(Utc::now()),
            duration_label: "0秒".to_string(),
            size_label: format_size_label(binding.size_bytes),
            status: "failed".to_string(),
        });

        error!(
            title = %binding.title,
            source = %binding.source_path.display(),
            "ebook conversion failed before upload: {error_message}"
        );
    }

    drop(store);
    emit_state_snapshot(app, state).await;
}

async fn refresh_all_devices(state: &KindleDesktopState) -> Result<(), DesktopBridgeError> {
    let usb_devices = state.usb_detector.scan_now()?;

    let usb_views = usb_devices
        .into_iter()
        .map(usb_device_to_view)
        .collect::<Vec<_>>();

    if state.upload_active.load(Ordering::SeqCst) && usb_views.is_empty() {
        return Ok(());
    }

    let mut store = state.store.write().await;
    store.replace_usb_devices(usb_views);
    Ok(())
}

async fn apply_usb_snapshot(state: &KindleDesktopState, devices: Vec<KindleDevice>) {
    let usb_views = devices
        .into_iter()
        .map(usb_device_to_view)
        .collect::<Vec<_>>();

    if state.upload_active.load(Ordering::SeqCst) && usb_views.is_empty() {
        return;
    }

    let mut store = state.store.write().await;
    store.replace_usb_devices(usb_views);
}

fn build_progress_callback(
    state: KindleDesktopState,
    app: AppHandle,
    bindings: Arc<Vec<QueueBinding>>,
) -> ProgressCallback {
    Arc::new(move |event: UploadProgressEvent| {
        let state = state.clone();
        let app = app.clone();
        let bindings = bindings.clone();

        tauri::async_runtime::spawn(async move {
            apply_upload_progress(&state, &app, bindings.as_ref(), &event).await;
        });
    })
}

async fn apply_upload_progress(
    state: &KindleDesktopState,
    app: &AppHandle,
    bindings: &[QueueBinding],
    event: &UploadProgressEvent,
) {
    let Some(binding) = bindings.get(event.item_index) else {
        return;
    };

    let mut store = state.store.write().await;
    let Some(queue_item) = store.queue_item_mut(&binding.queue_id) else {
        return;
    };

    if is_terminal_queue_stage(&queue_item.stage) {
        return;
    }

    match event.stage {
        UploadStage::SelectingStrategy => {
            queue_item.stage = "converting".to_string();
            queue_item.convert_progress = queue_item.convert_progress.max(10);
        }
        UploadStage::Scanning => {
            queue_item.stage = "converting".to_string();
            queue_item.convert_progress = queue_item.convert_progress.max(28);
        }
        UploadStage::Preparing => {
            queue_item.stage = "converting".to_string();
            queue_item.convert_progress = queue_item.convert_progress.max(72);
        }
        UploadStage::Uploading => {
            queue_item.stage = "uploading".to_string();
            queue_item.convert_progress = 100;

            let bytes_before = cumulative_bytes_before(bindings, event.item_index);
            let item_total = binding.size_bytes.max(1);
            let item_bytes = event
                .bytes_transferred
                .saturating_sub(bytes_before)
                .min(item_total);

            queue_item.upload_progress = percentage(item_bytes, item_total);
        }
        UploadStage::Completed => {
            queue_item.stage = "verifying".to_string();
            queue_item.convert_progress = 100;
            queue_item.upload_progress = 100;
        }
        UploadStage::Failed => {
            queue_item.stage = "failed".to_string();
            queue_item.destination_path = None;
        }
    }

    drop(store);
    emit_state_snapshot(app, state).await;
}

async fn finalize_upload_result(
    state: &KindleDesktopState,
    app: &AppHandle,
    bindings: &[QueueBinding],
    result: &UploadResult,
) {
    let mut store = state.store.write().await;

    for (binding, item_result) in bindings.iter().zip(result.items.iter()) {
        if let Some(queue_item) = store.queue_item_mut(&binding.queue_id) {
            apply_final_item_status(queue_item, item_result);
        }

        store.prepend_history(build_history_record(binding, item_result, result));
    }

    drop(store);
    emit_state_snapshot(app, state).await;
}

async fn fail_upload_batch(
    state: &KindleDesktopState,
    app: &AppHandle,
    bindings: &[QueueBinding],
    error_message: &str,
) {
    let mut store = state.store.write().await;

    for binding in bindings {
        if let Some(queue_item) = store.queue_item_mut(&binding.queue_id) {
            queue_item.stage = "failed".to_string();
            queue_item.convert_progress = queue_item.convert_progress.max(100);
        }

        store.prepend_history(HistoryRecordView {
            id: Uuid::new_v4().to_string(),
            title: binding.title.clone(),
            device_name: binding.device_name.clone(),
            connection: binding.connection.clone(),
            output_format: binding.output_format.clone(),
            transferred_at: format_history_timestamp(Utc::now()),
            duration_label: "0秒".to_string(),
            size_label: format_size_label(binding.size_bytes),
            status: "failed".to_string(),
        });
    }

    error!("upload batch failed: {error_message}");
    drop(store);
    emit_state_snapshot(app, state).await;
}

fn build_upload_target(device: &KindleDeviceView) -> UploadTarget {
    let mount_path = device.mount_path.clone().unwrap_or_default();
    UploadTarget::Usb(UsbUploadTarget::new(mount_path))
}

fn apply_final_item_status(queue_item: &mut UploadQueueItemView, item_result: &UploadItemResult) {
    match item_result.status {
        UploadItemStatus::Uploaded => {
            queue_item.stage = "done".to_string();
            queue_item.convert_progress = 100;
            queue_item.upload_progress = 100;
            queue_item.destination_path = Some(item_result.destination.clone());
        }
        UploadItemStatus::Skipped => {
            queue_item.stage = "failed".to_string();
            queue_item.convert_progress = 100;
            queue_item.destination_path = None;
        }
        UploadItemStatus::Failed => {
            queue_item.stage = "failed".to_string();
            queue_item.destination_path = None;
        }
    }
}

fn build_history_record(
    binding: &QueueBinding,
    item_result: &UploadItemResult,
    result: &UploadResult,
) -> HistoryRecordView {
    let status = match item_result.status {
        UploadItemStatus::Uploaded => "success",
        UploadItemStatus::Skipped => "partial",
        UploadItemStatus::Failed => "failed",
    };
    let transferred_at = format_history_timestamp(result.finished_at);
    let duration = result
        .finished_at
        .signed_duration_since(result.started_at)
        .to_std()
        .unwrap_or_default();

    HistoryRecordView {
        id: Uuid::new_v4().to_string(),
        title: binding.title.clone(),
        device_name: binding.device_name.clone(),
        connection: binding.connection.clone(),
        output_format: binding.output_format.clone(),
        transferred_at,
        duration_label: format_duration(duration),
        size_label: format_size_label(item_result.bytes_transferred.max(binding.size_bytes)),
        status: status.to_string(),
    }
}

fn usb_device_to_view(device: KindleDevice) -> KindleDeviceView {
    KindleDeviceView {
        id: device.id,
        name: device.name,
        model: device
            .model
            .unwrap_or_else(|| "Kindle USB 存储".to_string()),
        firmware: device.firmware.unwrap_or_else(|| "未知".to_string()),
        connection: "USB".to_string(),
        status: "ready".to_string(),
        upload_available: true,
        battery_level: 100,
        storage_used_gb: 0.0,
        storage_total_gb: DEFAULT_STORAGE_TOTAL_GB,
        ip_address: None,
        mount_path: Some(device.mount_path),
        supported_formats: vec![
            "AZW3".to_string(),
            "MOBI".to_string(),
            "EPUB".to_string(),
            "PDF".to_string(),
        ],
        last_seen_label: "刚刚挂载".to_string(),
        endpoint: None,
    }
}

async fn emit_state_snapshot(app: &AppHandle, state: &KindleDesktopState) {
    if let Err(error) = app.emit(STATE_EVENT_NAME, state.snapshot().await) {
        warn!("failed to emit state snapshot: {error}");
    }
}

fn preferred_target_format(_device: &KindleDeviceView) -> String {
    "AZW3".to_string()
}

fn same_queue_source(left: &UploadQueueItemView, right: &UploadQueueItemView) -> bool {
    left.device_id == right.device_id
        && normalized_source_path(&left.source_path) == normalized_source_path(&right.source_path)
}

fn is_active_queue_stage(stage: &str) -> bool {
    matches!(stage, "converting" | "uploading" | "verifying")
}

fn is_terminal_queue_stage(stage: &str) -> bool {
    matches!(stage, "done" | "failed")
}

fn normalized_source_path(value: &str) -> String {
    let normalized = value.replace('\\', "/");

    #[cfg(target_os = "windows")]
    {
        normalized.to_ascii_lowercase()
    }

    #[cfg(not(target_os = "windows"))]
    {
        normalized
    }
}

fn file_extension_uppercase(path: &PathBuf) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_uppercase())
        .unwrap_or_else(|| "FILE".to_string())
}

fn percentage(current: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }

    let ratio = (current as f64 / total as f64) * 100.0;
    ratio.clamp(0.0, 100.0).round() as u8
}

fn cumulative_bytes_before(bindings: &[QueueBinding], item_index: usize) -> u64 {
    bindings
        .iter()
        .take(item_index)
        .map(|binding| binding.size_bytes)
        .sum()
}

fn round_size_mb(bytes: u64) -> f64 {
    ((bytes as f64 / 1024.0 / 1024.0) * 10.0).round() / 10.0
}

fn format_size_label(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
}

fn format_duration(duration: std::time::Duration) -> String {
    if duration.as_secs() < 60 {
        format!("{}秒", duration.as_secs())
    } else {
        let minutes = duration.as_secs() / 60;
        let seconds = duration.as_secs() % 60;
        format!("{minutes}分{seconds:02}秒")
    }
}

fn format_history_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}
