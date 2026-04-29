//! Upload strategies for delivering ebooks to a Kindle.
//!
//! This module exposes a strategy-based upload pipeline focused on direct USB
//! delivery:
//!
//! - USB copy to `Kindle/documents/`
//!
//! The shared types below intentionally keep transport-specific configuration
//! separate from upload-job state. That separation makes the module easy to
//! integrate with a future device manager and a persistent transfer-history
//! store.

pub mod kindle_thumbnail;
pub mod upload_manager;
pub mod usb_upload;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

pub use upload_manager::UploadManager;
pub use usb_upload::UsbUploadStrategy;

const DEFAULT_USB_DOCUMENTS_DIR: &str = "documents";

/// Callback type used for upload progress notifications.
pub type ProgressCallback = Arc<dyn Fn(UploadProgressEvent) + Send + Sync>;

/// High-level transport identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UploadKind {
    /// Direct filesystem copy over USB mass storage.
    Usb,
}

/// Progress phases emitted during upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadStage {
    /// The manager is choosing a strategy.
    SelectingStrategy,
    /// The selected strategy is discovering or resolving a device endpoint.
    Scanning,
    /// The strategy is preparing a file for transfer.
    Preparing,
    /// The strategy is actively transferring the current file.
    Uploading,
    /// The current file or the whole job completed successfully.
    Completed,
    /// The current file or the whole job failed.
    Failed,
}

/// Overall upload status returned to callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadStatus {
    /// Every requested file uploaded successfully.
    Success,
    /// At least one file uploaded successfully and at least one failed or was
    /// skipped.
    PartialSuccess,
    /// No file was uploaded successfully.
    Failed,
}

/// Per-file upload status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadItemStatus {
    /// File uploaded successfully.
    Uploaded,
    /// File was intentionally skipped, usually because the destination already
    /// exists and overwrite is disabled.
    Skipped,
    /// File upload failed.
    Failed,
}

/// Upload progress payload delivered to the caller callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadProgressEvent {
    /// Selected transport when known.
    pub strategy: Option<UploadKind>,
    /// Current stage of the upload flow.
    pub stage: UploadStage,
    /// Zero-based item index.
    pub item_index: usize,
    /// Total number of items in the job.
    pub total_items: usize,
    /// Current item display name when applicable.
    pub current_item: Option<String>,
    /// Bytes transferred so far for the whole job.
    pub bytes_transferred: u64,
    /// Total known bytes for the whole job.
    pub total_bytes: u64,
    /// Human-readable detail for UI and logs.
    pub message: Option<String>,
}

/// Input file metadata for an upload job.
#[derive(Debug, Clone)]
pub struct UploadItem {
    /// Absolute or relative source file path on disk.
    pub source_path: PathBuf,
    /// Optional destination file name override.
    pub file_name: Option<String>,
    /// Optional MIME type override reserved for future transport adapters.
    pub mime_type: Option<String>,
}

/// High-level upload request.
#[derive(Debug, Clone)]
pub struct UploadRequest {
    /// Destination configuration.
    pub target: UploadTarget,
    /// Files to upload.
    pub items: Vec<UploadItem>,
    /// Whether existing destination files may be overwritten.
    pub overwrite: bool,
}

/// Upload target variants understood by the manager.
#[derive(Debug, Clone)]
pub enum UploadTarget {
    /// Explicit USB upload target.
    Usb(UsbUploadTarget),
    /// Automatic selection from the available transport descriptors.
    Auto(AutoUploadTarget),
}

/// Explicit USB upload configuration.
#[derive(Debug, Clone)]
pub struct UsbUploadTarget {
    /// Mounted Kindle root path.
    pub mount_path: PathBuf,
    /// Documents directory name inside the Kindle mount.
    pub documents_dir_name: String,
}

impl UsbUploadTarget {
    /// Construct a USB target using the default Kindle `documents` directory.
    pub fn new(mount_path: impl Into<PathBuf>) -> Self {
        Self {
            mount_path: mount_path.into(),
            documents_dir_name: DEFAULT_USB_DOCUMENTS_DIR.to_string(),
        }
    }

    /// Resolve the effective `documents` destination directory.
    pub fn documents_path(&self) -> PathBuf {
        self.mount_path.join(&self.documents_dir_name)
    }
}

/// Automatic upload configuration.
#[derive(Debug, Clone, Default)]
pub struct AutoUploadTarget {
    /// USB candidate used when the Kindle is mounted locally.
    pub usb: Option<UsbUploadTarget>,
}

/// Per-file upload result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadItemResult {
    /// Source file path.
    pub source_path: PathBuf,
    /// Destination description, usually the final Kindle filesystem path.
    pub destination: String,
    /// Effective file name used by the strategy.
    pub file_name: String,
    /// Per-file status.
    pub status: UploadItemStatus,
    /// Bytes transferred for this item.
    pub bytes_transferred: u64,
    /// Failure detail when status is `Failed`.
    pub error: Option<String>,
}

/// Aggregate upload result returned to the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadResult {
    /// Selected strategy.
    pub strategy: UploadKind,
    /// Overall job status.
    pub status: UploadStatus,
    /// Per-file outcomes.
    pub items: Vec<UploadItemResult>,
    /// Total bytes transferred successfully.
    pub total_bytes_transferred: u64,
    /// Start timestamp.
    pub started_at: DateTime<Utc>,
    /// End timestamp.
    pub finished_at: DateTime<Utc>,
}

/// Shared upload error type.
#[derive(Debug, Error)]
pub enum UploadError {
    #[error("upload request is invalid: {0}")]
    InvalidRequest(String),
    #[error("no upload strategy is available for the requested connection")]
    NoAvailableStrategy,
    #[error("requested upload strategy {0:?} is not available")]
    StrategyUnavailable(UploadKind),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("destination already exists and overwrite is disabled: {0}")]
    DestinationExists(String),
    #[error("transport rejected upload: {0}")]
    TransportRejected(String),
}

/// Strategy interface implemented by all upload transports.
#[async_trait]
pub trait UploadStrategy: Send + Sync {
    /// Strategy kind.
    fn kind(&self) -> UploadKind;

    /// Check whether this strategy is applicable to the request.
    async fn probe(&self, request: &UploadRequest) -> Result<bool, UploadError>;

    /// Execute the upload.
    async fn upload(
        &self,
        request: &UploadRequest,
        reporter: ProgressReporter,
    ) -> Result<UploadResult, UploadError>;
}

/// Lightweight wrapper around an optional progress callback.
#[derive(Clone, Default)]
pub struct ProgressReporter {
    callback: Option<ProgressCallback>,
}

impl ProgressReporter {
    /// Construct a reporter from an optional callback.
    pub fn new(callback: Option<ProgressCallback>) -> Self {
        Self { callback }
    }

    /// Emit a progress event.
    pub fn emit(&self, event: UploadProgressEvent) {
        if let Some(callback) = &self.callback {
            callback(event);
        }
    }
}

impl UploadTarget {
    /// Returns the USB target if this request can use USB upload.
    pub(crate) fn usb_target(&self) -> Option<&UsbUploadTarget> {
        match self {
            Self::Usb(target) => Some(target),
            Self::Auto(target) => target.usb.as_ref(),
        }
    }
}

/// Determine the effective overall status from per-item results.
pub(crate) fn summarize_status(items: &[UploadItemResult]) -> UploadStatus {
    let mut uploaded = 0usize;
    let mut not_uploaded = 0usize;

    for item in items {
        match item.status {
            UploadItemStatus::Uploaded => uploaded += 1,
            UploadItemStatus::Skipped | UploadItemStatus::Failed => not_uploaded += 1,
        }
    }

    match (uploaded, not_uploaded) {
        (0, _) => UploadStatus::Failed,
        (_, 0) => UploadStatus::Success,
        _ => UploadStatus::PartialSuccess,
    }
}

/// Resolve the effective output file name for an upload item.
pub(crate) fn resolve_file_name(item: &UploadItem) -> Result<String, UploadError> {
    let candidate = item
        .file_name
        .clone()
        .or_else(|| {
            item.source_path
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .ok_or_else(|| {
            UploadError::InvalidRequest(format!(
                "source path has no file name: {}",
                item.source_path.display()
            ))
        })?;
    let sanitized = sanitize_file_name(&candidate);

    if sanitized.is_empty() {
        return Err(UploadError::InvalidRequest(format!(
            "invalid destination file name derived from {}",
            item.source_path.display()
        )));
    }

    Ok(sanitized)
}

/// Sanitize a file name so strategies cannot accidentally write outside the
/// intended destination directory.
pub(crate) fn sanitize_file_name(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => character,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Build a normalized human-readable path string for results and errors.
pub(crate) fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Compute the sum of file sizes that currently exist on disk.
pub(crate) async fn total_known_bytes(items: &[UploadItem]) -> u64 {
    let mut total = 0_u64;

    for item in items {
        if let Ok(metadata) = tokio::fs::metadata(&item.source_path).await {
            total = total.saturating_add(metadata.len());
        }
    }

    total
}

/// Emit a convenience progress event.
pub(crate) fn emit_progress(
    reporter: &ProgressReporter,
    strategy: Option<UploadKind>,
    stage: UploadStage,
    item_index: usize,
    total_items: usize,
    current_item: Option<String>,
    bytes_transferred: u64,
    total_bytes: u64,
    message: impl Into<Option<String>>,
) {
    reporter.emit(UploadProgressEvent {
        strategy,
        stage,
        item_index,
        total_items,
        current_item,
        bytes_transferred,
        total_bytes,
        message: message.into(),
    });
}
