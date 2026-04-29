//! USB upload strategy.
//!
//! This strategy performs direct filesystem copies into `Kindle/documents/`.

use super::{
    display_path, emit_progress, kindle_thumbnail::KindleThumbnailService, resolve_file_name,
    summarize_status, total_known_bytes, ProgressReporter, UploadError, UploadItemResult,
    UploadItemStatus, UploadKind, UploadRequest, UploadResult, UploadStage, UploadStrategy,
};
use async_trait::async_trait;
use chrono::Utc;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::warn;

const COPY_BUFFER_SIZE: usize = 64 * 1024;

/// Concrete USB upload strategy.
#[derive(Debug, Clone, Default)]
pub struct UsbUploadStrategy;

#[async_trait]
impl UploadStrategy for UsbUploadStrategy {
    fn kind(&self) -> UploadKind {
        UploadKind::Usb
    }

    async fn probe(&self, request: &UploadRequest) -> Result<bool, UploadError> {
        let Some(target) = request.target.usb_target() else {
            return Ok(false);
        };

        match fs::metadata(&target.mount_path).await {
            Ok(metadata) => Ok(metadata.is_dir()),
            Err(_) => Ok(false),
        }
    }

    async fn upload(
        &self,
        request: &UploadRequest,
        reporter: ProgressReporter,
    ) -> Result<UploadResult, UploadError> {
        let target = request
            .target
            .usb_target()
            .ok_or(UploadError::StrategyUnavailable(UploadKind::Usb))?;
        let started_at = Utc::now();
        let total_bytes = total_known_bytes(&request.items).await;
        let total_items = request.items.len();
        let documents_path = target.documents_path();
        let thumbnail_service = KindleThumbnailService;

        fs::create_dir_all(&documents_path).await?;
        if let Err(error) = thumbnail_service
            .sync_cached_thumbnails(&target.mount_path)
            .await
        {
            warn!(
                mount = %display_path(&target.mount_path),
                "failed to restore cached Kindle thumbnails: {error}"
            );
        }

        let mut cumulative_bytes = 0_u64;
        let mut item_results = Vec::with_capacity(total_items);

        for (item_index, item) in request.items.iter().enumerate() {
            let file_name = match resolve_file_name(item) {
                Ok(file_name) => file_name,
                Err(error) => {
                    item_results.push(UploadItemResult {
                        source_path: item.source_path.clone(),
                        destination: display_path(&documents_path),
                        file_name: String::new(),
                        status: UploadItemStatus::Failed,
                        bytes_transferred: 0,
                        error: Some(error.to_string()),
                    });
                    continue;
                }
            };
            let destination_path = documents_path.join(&file_name);

            emit_progress(
                &reporter,
                Some(self.kind()),
                UploadStage::Preparing,
                item_index,
                total_items,
                Some(file_name.clone()),
                cumulative_bytes,
                total_bytes,
                Some(format!(
                    "Preparing USB transfer to {}",
                    display_path(&destination_path)
                )),
            );

            let result = match copy_item_to_usb(
                item,
                &destination_path,
                request.overwrite,
                &reporter,
                item_index,
                total_items,
                total_bytes,
                cumulative_bytes,
            )
            .await
            {
                Ok(bytes_transferred) => {
                    if let Err(error) = thumbnail_service
                        .upload_thumbnail_for_book(&destination_path, &target.mount_path)
                        .await
                    {
                        warn!(
                            destination = %display_path(&destination_path),
                            "failed to upload Kindle cover thumbnail: {error}"
                        );
                    }

                    cumulative_bytes = cumulative_bytes.saturating_add(bytes_transferred);
                    UploadItemResult {
                        source_path: item.source_path.clone(),
                        destination: display_path(&destination_path),
                        file_name: file_name.clone(),
                        status: UploadItemStatus::Uploaded,
                        bytes_transferred,
                        error: None,
                    }
                }
                Err(UploadError::DestinationExists(message)) => UploadItemResult {
                    source_path: item.source_path.clone(),
                    destination: display_path(&destination_path),
                    file_name: file_name.clone(),
                    status: UploadItemStatus::Skipped,
                    bytes_transferred: 0,
                    error: Some(message),
                },
                Err(error) => UploadItemResult {
                    source_path: item.source_path.clone(),
                    destination: display_path(&destination_path),
                    file_name: file_name.clone(),
                    status: UploadItemStatus::Failed,
                    bytes_transferred: 0,
                    error: Some(error.to_string()),
                },
            };

            let stage = match result.status {
                UploadItemStatus::Uploaded => UploadStage::Completed,
                UploadItemStatus::Skipped | UploadItemStatus::Failed => UploadStage::Failed,
            };
            emit_progress(
                &reporter,
                Some(self.kind()),
                stage,
                item_index,
                total_items,
                Some(file_name.clone()),
                cumulative_bytes,
                total_bytes,
                result
                    .error
                    .clone()
                    .or_else(|| Some(format!("USB transfer finished for {}", result.file_name))),
            );

            item_results.push(result);
        }

        Ok(UploadResult {
            strategy: self.kind(),
            status: summarize_status(&item_results),
            items: item_results,
            total_bytes_transferred: cumulative_bytes,
            started_at,
            finished_at: Utc::now(),
        })
    }
}

async fn copy_item_to_usb(
    item: &super::UploadItem,
    destination_path: &std::path::Path,
    overwrite: bool,
    reporter: &ProgressReporter,
    item_index: usize,
    total_items: usize,
    total_bytes: u64,
    base_bytes_transferred: u64,
) -> Result<u64, UploadError> {
    let source_metadata = fs::metadata(&item.source_path).await?;
    let source_size = source_metadata.len();

    if destination_path.exists() && !overwrite {
        return Err(UploadError::DestinationExists(display_path(
            destination_path,
        )));
    }

    let mut source = File::open(&item.source_path).await?;
    let mut destination = if overwrite {
        OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(destination_path)
            .await?
    } else {
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(destination_path)
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    UploadError::DestinationExists(display_path(destination_path))
                } else {
                    UploadError::Io(error)
                }
            })?
    };

    let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
    let mut item_bytes_transferred = 0_u64;

    loop {
        let bytes_read = match source.read(&mut buffer).await {
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                let _ = fs::remove_file(destination_path).await;
                return Err(UploadError::Io(error));
            }
        };
        if bytes_read == 0 {
            break;
        }

        if let Err(error) = destination.write_all(&buffer[..bytes_read]).await {
            let _ = fs::remove_file(destination_path).await;
            return Err(UploadError::Io(error));
        }
        item_bytes_transferred = item_bytes_transferred.saturating_add(bytes_read as u64);

        emit_progress(
            reporter,
            Some(UploadKind::Usb),
            UploadStage::Uploading,
            item_index,
            total_items,
            item.source_path
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_string),
            base_bytes_transferred.saturating_add(item_bytes_transferred),
            total_bytes,
            Some(format!(
                "Copied {} / {} bytes over USB",
                item_bytes_transferred, source_size
            )),
        );
    }

    if let Err(error) = destination.flush().await {
        let _ = fs::remove_file(destination_path).await;
        return Err(UploadError::Io(error));
    }

    if let Err(error) = destination.sync_all().await {
        let _ = fs::remove_file(destination_path).await;
        return Err(UploadError::Io(error));
    }

    let written_size = match fs::metadata(destination_path).await {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            let _ = fs::remove_file(destination_path).await;
            return Err(UploadError::Io(error));
        }
    };

    if written_size != source_size {
        let _ = fs::remove_file(destination_path).await;
        return Err(UploadError::TransportRejected(format!(
            "USB copy verification failed for {}: expected {} bytes, wrote {} bytes",
            display_path(destination_path),
            source_size,
            written_size
        )));
    }

    Ok(item_bytes_transferred)
}
