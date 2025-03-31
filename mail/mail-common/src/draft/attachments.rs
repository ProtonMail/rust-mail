use crate::MailUserContext;
use crate::datatypes::AttachmentMetadata;
use crate::models::{
    DraftAttachmentMetadata, DraftAttachmentUploadError, DraftAttachmentUploadState, DraftMetadata,
    MetadataId,
};
use proton_mail_ids::LocalAttachmentId;
use stash::stash::{StashError, Tether};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::ReadDir;
use tracing::log::trace;
use tracing::{Instrument, debug, debug_span, error};

/// Represent an attachment associated with a draft.
pub struct DraftAttachment {
    /// Metadata of the attachment.
    pub metadata: AttachmentMetadata,
    /// Upload status.
    pub state: DraftAttachmentState,
    /// Timestamp of the state update. It will be 0 for attachment that already exist.
    pub state_modified_timestamp: i64,
}

#[derive(Debug)]
pub enum DraftAttachmentState {
    /// Attachment has not been uploaded.
    Uploading,
    /// Attachment has been uploaded to the server
    Uploaded,
    /// Attachment failed to upload or encrypt.
    Error(DraftAttachmentUploadError),
    /// Could not upload due to lack of network,
    Offline,
    /// Attachment is awaiting upload.
    Pending,
}

impl DraftAttachmentState {
    pub fn from_draft_attachment_metadata(metadata: &DraftAttachmentMetadata) -> Self {
        match metadata.state() {
            DraftAttachmentUploadState::Uploading => Self::Uploading,
            DraftAttachmentUploadState::Uploaded => Self::Uploaded,
            DraftAttachmentUploadState::Pending => Self::Pending,
            DraftAttachmentUploadState::Error => {
                let error = metadata
                    .error
                    .clone()
                    .unwrap_or(DraftAttachmentUploadError::Unexpected);
                Self::Error(error)
            }
            DraftAttachmentUploadState::Offline => Self::Offline,
        }
    }
}

impl DraftAttachment {
    /// Merge the list of `attachments` with the attachment data associated with the draft
    /// with `metadata_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn build_list(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<Vec<DraftAttachment>, StashError> {
        let attachments =
            DraftAttachmentMetadata::attachment_for_draft(metadata_id, tether).await?;
        let metadata_map: HashMap<LocalAttachmentId, DraftAttachmentMetadata> = HashMap::from_iter(
            DraftAttachmentMetadata::find_by_metadata_id(metadata_id, tether)
                .await?
                .into_iter()
                .map(|m| (m.local_attachment_id, m)),
        );

        Ok(attachments
            .into_iter()
            .filter_map(|attachment| {
                let (state, timestamp) =
                    if let Some(metadata) = metadata_map.get(&attachment.local_id.unwrap()) {
                        if metadata.deleted {
                            return None;
                        }
                        (
                            DraftAttachmentState::from_draft_attachment_metadata(metadata),
                            metadata.state_timestamp(),
                        )
                    } else {
                        // If there is no metadata entry, it means there are no changes for this attachment
                        // or it was inherited from a reply/forward.
                        (DraftAttachmentState::Uploaded, 0)
                    };
                Some(DraftAttachment {
                    state,
                    metadata: AttachmentMetadata::from(attachment),
                    state_modified_timestamp: timestamp,
                })
            })
            .collect())
    }
}

/// Background cleaner task to clean up the draft staging area from time to time.
///
/// The staging is cleared opportunistically whenever possible. It is possible that
/// due to permission errors or exception code paths that the cleanup code can not
/// be run. For these instances we periodically try to remove leftover files.
#[derive(Debug, Default)]
pub struct DraftStagingAreaCleaner {
    interval: Duration,
}

const DEFAULT_CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 10); // 10min.
impl DraftStagingAreaCleaner {
    /// Create a new instance which will run the deferred cleanup with default interval of
    /// 10 min.
    pub fn new() -> Self {
        Self::with_interval(DEFAULT_CLEANUP_INTERVAL)
    }

    /// Create a new instance which will run the deferred cleanup every at `interval`.
    pub fn with_interval(interval: Duration) -> Self {
        Self { interval }
    }

    /// Start the cleaner background task.
    ///
    /// We also create the staging area directory if it does not exist yet.
    ///
    /// # Errors
    ///
    /// If we failed to create the staging area.
    pub fn run(self, context: Arc<MailUserContext>) -> std::io::Result<()> {
        let staging_area = context.attachment_staging_path();
        std::fs::create_dir_all(&staging_area)
            .inspect_err(|e| error!("failed to create draft staging area: {e:?}"))?;

        let weak_context = Arc::downgrade(&context);
        context.spawn(
            async move {
                loop {
                    let Some(ctx) = weak_context.upgrade() else {
                        return;
                    };
                    debug!("Starting draft staging cleanup");
                    match tokio::fs::read_dir(&staging_area).await {
                        Ok(dir_reader) => {
                            let tether = ctx.user_stash().connection();
                            Self::run_cleanup(dir_reader, &tether).await;
                            drop(tether);
                        }
                        Err(e) => {
                            error!("Failed to open draft staging dir {staging_area:?}: {e:?}");
                        }
                    };
                    drop(ctx);
                    tokio::time::sleep(self.interval).await;
                }
            }
            .instrument(debug_span!("draft-staging-cleanup")),
        );
        Ok(())
    }

    async fn run_cleanup(mut dir_reader: ReadDir, tether: &Tether) {
        while let Some(entry) = match dir_reader.next_entry().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to read dir entry: {e:?}");
                return;
            }
        } {
            trace!("Checking {:?}", entry.path());
            let Ok(file_type) = entry.file_type().await else {
                error!(
                    "Failed to get file type for {:?}, skipping...",
                    entry.path()
                );
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }

            // check if file is a valid integer.
            let entry_file_name = entry.file_name();
            let entry_str = entry_file_name.to_string_lossy();
            let Ok(id) = entry_str.parse::<u64>().map(MetadataId) else {
                trace!("Entry '{entry_str}' is not a valid metadata id, skipping...");
                continue;
            };

            // Check if metadata file is still present, if not we can delete the directory.
            let Ok(None) = DraftMetadata::find_by_id(id, tether)
                .await
                .inspect_err(|e| error!("Failed to load draft metadata for {id}: {e:?}"))
            else {
                continue;
            };

            debug!("Removing {:?}", entry.path());
            if let Err(e) = tokio::fs::remove_dir_all(entry.path()).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    error!("Failed to remove draft staging dir {entry_str}: {e:?}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DraftMetadata;
    use proton_mail_test_utils::db::new_test_connection_file;

    #[tokio::test]
    async fn background_cleaner() {
        let (stash, _db_dir) = new_test_connection_file().await;
        let tmpdir = tempfile::tempdir().unwrap();

        let mut tether = stash.connection();
        let db_metadata = tether
            .tx(async |tx| DraftMetadata::empty(tx).await)
            .await
            .unwrap();

        let staging_path = tmpdir.path();
        let staging_path_metadata_1 = staging_path.join(db_metadata.id.unwrap().0.to_string());
        let staging_path_metadata_2 = staging_path.join("10");
        let staging_path_metadata_3 = staging_path.join("4");

        let metadata_1_file = staging_path_metadata_1.join("hello_world.txt");
        let metadata_2_file_1 = staging_path_metadata_2.join("hello_world_2.txt");
        let metadata_2_file_2 = staging_path_metadata_2.join("hello_world_m2.txt");
        let metadata_3_file = staging_path_metadata_3.join("hello_world_m3.txt");

        // create directories and write files
        tokio::fs::create_dir_all(&staging_path).await.unwrap();
        tokio::fs::create_dir_all(&staging_path_metadata_1)
            .await
            .unwrap();
        tokio::fs::create_dir_all(&staging_path_metadata_2)
            .await
            .unwrap();
        tokio::fs::create_dir_all(&staging_path_metadata_3)
            .await
            .unwrap();

        tokio::fs::write(&metadata_1_file, "hello metadata 1")
            .await
            .unwrap();
        tokio::fs::write(&metadata_2_file_2, "hello metadata 2-2")
            .await
            .unwrap();
        tokio::fs::write(&metadata_2_file_1, "hello metadata 2")
            .await
            .unwrap();
        tokio::fs::write(&metadata_3_file, "hello metadata 2")
            .await
            .unwrap();

        // run cleanup
        let read_dir = tokio::fs::read_dir(&staging_path).await.unwrap();
        DraftStagingAreaCleaner::run_cleanup(read_dir, &tether).await;

        // Assert files are removed correctly.
        assert!(metadata_1_file.exists());
        assert!(staging_path_metadata_1.exists());
        assert!(!metadata_2_file_1.exists());
        assert!(!metadata_2_file_2.exists());
        assert!(!staging_path_metadata_2.exists());
        assert!(!metadata_3_file.exists());
        assert!(!staging_path_metadata_3.exists());
    }
}
