//! Upload manager that chooses and executes the appropriate strategy.

use super::{
    emit_progress, total_known_bytes, ProgressCallback, ProgressReporter, UploadError, UploadKind,
    UploadRequest, UploadResult, UploadStage, UploadStatus, UploadStrategy, UsbUploadStrategy,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Orchestrates strategy selection and upload execution.
#[derive(Clone)]
pub struct UploadManager {
    strategies: HashMap<UploadKind, Arc<dyn UploadStrategy>>,
    auto_priority: Vec<UploadKind>,
}

impl Default for UploadManager {
    fn default() -> Self {
        let usb: Arc<dyn UploadStrategy> = Arc::new(UsbUploadStrategy::default());

        Self::new(usb)
    }
}

impl UploadManager {
    /// Construct a manager from the concrete USB strategy implementation.
    pub fn new(usb: Arc<dyn UploadStrategy>) -> Self {
        let mut strategies = HashMap::new();
        strategies.insert(UploadKind::Usb, usb);

        Self {
            strategies,
            auto_priority: vec![UploadKind::Usb],
        }
    }

    /// Change the default auto-selection order.
    pub fn with_auto_priority(mut self, auto_priority: Vec<UploadKind>) -> Self {
        self.auto_priority = auto_priority;
        self
    }

    /// Execute an upload request using the best available strategy.
    pub async fn upload(
        &self,
        request: UploadRequest,
        callback: Option<ProgressCallback>,
    ) -> Result<UploadResult, UploadError> {
        if request.items.is_empty() {
            return Err(UploadError::InvalidRequest(
                "upload request must contain at least one item".to_string(),
            ));
        }

        let total_bytes = total_known_bytes(&request.items).await;
        let total_items = request.items.len();
        let reporter = ProgressReporter::new(callback);

        emit_progress(
            &reporter,
            None,
            UploadStage::SelectingStrategy,
            0,
            total_items,
            None,
            0,
            total_bytes,
            Some("Selecting upload strategy".to_string()),
        );

        let strategy = match self.select_strategy(&request).await {
            Ok(strategy) => strategy,
            Err(error) => {
                emit_progress(
                    &reporter,
                    None,
                    UploadStage::Failed,
                    0,
                    total_items,
                    None,
                    0,
                    total_bytes,
                    Some(error.to_string()),
                );
                return Err(error);
            }
        };
        let result = match strategy.upload(&request, reporter.clone()).await {
            Ok(result) => result,
            Err(error) => {
                emit_progress(
                    &reporter,
                    Some(strategy.kind()),
                    UploadStage::Failed,
                    0,
                    total_items,
                    None,
                    0,
                    total_bytes,
                    Some(error.to_string()),
                );
                return Err(error);
            }
        };

        emit_progress(
            &reporter,
            Some(result.strategy),
            match result.status {
                UploadStatus::Success | UploadStatus::PartialSuccess => UploadStage::Completed,
                UploadStatus::Failed => UploadStage::Failed,
            },
            total_items.saturating_sub(1),
            total_items,
            None,
            result.total_bytes_transferred,
            total_bytes,
            Some(format!("Upload finished with status {:?}", result.status)),
        );

        Ok(result)
    }

    async fn select_strategy(
        &self,
        request: &UploadRequest,
    ) -> Result<Arc<dyn UploadStrategy>, UploadError> {
        match &request.target {
            super::UploadTarget::Usb(_) => {
                self.require_explicit_strategy(UploadKind::Usb, request)
                    .await
            }
            super::UploadTarget::Auto(_) => {
                for kind in &self.auto_priority {
                    let strategy = self
                        .strategies
                        .get(kind)
                        .cloned()
                        .ok_or(UploadError::StrategyUnavailable(*kind))?;

                    if strategy.probe(request).await? {
                        return Ok(strategy);
                    }
                }

                Err(UploadError::NoAvailableStrategy)
            }
        }
    }

    async fn require_explicit_strategy(
        &self,
        kind: UploadKind,
        request: &UploadRequest,
    ) -> Result<Arc<dyn UploadStrategy>, UploadError> {
        let strategy = self
            .strategies
            .get(&kind)
            .cloned()
            .ok_or(UploadError::StrategyUnavailable(kind))?;

        if !strategy.probe(request).await? {
            return Err(UploadError::StrategyUnavailable(kind));
        }

        Ok(strategy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uploader::{AutoUploadTarget, UploadItem, UploadTarget, UsbUploadTarget};
    use tempfile::tempdir;

    #[tokio::test]
    async fn auto_selection_prefers_usb_when_mount_exists() {
        let temp = tempdir().expect("tempdir");
        let mount_path = temp.path().join("Kindle");
        let documents_path = mount_path.join("documents");
        let source_path = temp.path().join("book.epub");

        tokio::fs::create_dir_all(&documents_path)
            .await
            .expect("create documents dir");
        tokio::fs::write(&source_path, b"dummy epub bytes")
            .await
            .expect("write source");

        let manager = UploadManager::default();
        let request = UploadRequest {
            target: UploadTarget::Auto(AutoUploadTarget {
                usb: Some(UsbUploadTarget::new(&mount_path)),
            }),
            items: vec![UploadItem {
                source_path: source_path.clone(),
                file_name: None,
                mime_type: None,
            }],
            overwrite: true,
        };

        let result = manager
            .upload(request, None)
            .await
            .expect("upload succeeds");

        assert_eq!(result.strategy, UploadKind::Usb);
        assert_eq!(result.status, UploadStatus::Success);
        assert!(documents_path.join("book.epub").exists());
    }
}
