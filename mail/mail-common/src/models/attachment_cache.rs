use super::Message;
use crate::datatypes::exclusive_location::ExclusiveLocation;
use crate::datatypes::{AttachmentMetadata, Disposition, LocalAttachmentId, SystemLabelId as _};
use crate::models::{Attachment, AttachmentType};
use crate::{AppError, DecryptedAttachment, MailContextError, MailContextResult, MailUserContext};
use anyhow::Context as _;
use indoc::indoc;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::ModelExtension as _;
use proton_core_common::os::{safe_write_async, sanitize_filename};
use proton_core_common::{DeleteFilesSafeError, Origin};
use proton_crypto_inbox::attachment::DecryptableAttachment as _;
use proton_crypto_inbox::proton_crypto::crypto::{
    PGPProvider, PGPProviderSync, VerificationResult,
};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use stash::exports::{SqliteError, ToSql};
use stash::macros::Model;
use stash::orm::Model as _;
use stash::params;
use stash::stash::RunTransaction;
use stash::stash::{Bond, StashError};
use stash::utils::placeholders_n;
use std::io::Read;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use tokio::fs;
use tracing::{debug, error, info, trace, warn};

/// This is the metadata for where or if the attachment has the data downloaded
/// It's stored in a separate table because:
/// 1. Attachments can not have data yet (or anymore).
/// 2. Data might still exist even if it hasn't been deleted yet.
///
/// It contains a bunch of fields useful for deciding whether or not to evict the attachment.
/// See the [`attachments`] module to see more details on how these are calculated
///
/// [`attachments`]: crate::mailbox::attachments
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("attachment_cache")]
pub struct AttachmentCacheMetadata {
    #[IdField]
    pub attachment_id: LocalAttachmentId,

    /// Last access time of the attachment.
    #[DbField]
    pub atime: u64,

    /// Creation time of the attachment. Currently unused.
    #[DbField]
    pub ctime: u64,

    /// How many times this attachment has been accessed. It starts at 0.
    #[DbField]
    pub hit_count: u64,

    #[DbField]
    pub path: String,

    /// The size of the attachment in bytes
    #[DbField]
    pub size: u64,
}

impl Attachment {
    /// Tries to get where an attachment is stored.
    ///
    /// a. Try to read it from the cache
    /// b.
    ///     - Download it if it can't find it
    ///     - Save it to disk
    ///     - Return the path as a String
    pub async fn content_path(
        &self,
        ctx: &MailUserContext,
        tx: &mut impl RunTransaction,
    ) -> MailContextResult<PathBuf> {
        if let Some(path) = Self::path_from_cache_and_update_metadata_atomic(self.id(), tx).await? {
            return Ok(path);
        };

        let data = self.fetch_data(ctx).await?;

        // While we were downlaoding, did someone win the race?
        // If so return it. Else store it.
        // TODO(orion): Replace this
        tx.run_tx(async |tx| {
            if let Some(path) = Self::path_from_cache_and_update_metadata(self.id(), tx).await? {
                debug!("Someone else won the race");
                return Ok(path);
            };

            Ok(Self::store_in_cache(ctx, &self.filename, self.id(), data, tx).await?)
        })
        .await
        .map_err(MailContextError::IntoTransactionError)
    }

    /// Tries to get the actual bytes of an attachment.
    ///
    /// a. Try to read it from the cache
    /// b.
    ///     - Download it if it can't find it
    ///     - Save it to disk
    ///     - Return the data.
    /// c. Trigger the [`Self::cleanup_attachment_cache`] background routine
    pub async fn content_data(
        &self,
        ctx: &MailUserContext,
        tx: &mut impl RunTransaction,
    ) -> MailContextResult<Vec<u8>> {
        if let Some(path) = Self::path_from_cache_and_update_metadata_atomic(self.id(), tx).await? {
            return Ok(fs::read(path).await?);
        };

        let data = self.fetch_data(ctx).await?;

        // While we were downlaoding, did someone win the race?
        // If so return it. Else store it.
        // TODO(orion): Replace this
        tx.run_tx(async |tx| {
            if let Some(path) = Self::path_from_cache_and_update_metadata(self.id(), tx).await? {
                return Ok(fs::read(path).await?);
            };

            if let Err(e) =
                Self::store_in_cache(ctx, &self.filename, self.id(), data.clone(), tx).await
            {
                error!("Could not save attachment to disk/database, but will continue: {e:?}");
            }
            Ok(data)
        })
        .await
        .map_err(MailContextError::IntoTransactionError)
    }

    /// Loads the metadata and file path for the given local [`attachment_id`]
    /// into a [`DecryptedAttachment`].
    ///
    /// If the attachment is not present on the device it is retrieved from
    /// the server, decrypted and stored in the cache.
    ///
    /// Additionally, attempts to verify any attached signatures with the
    /// sender's keys. The result can be accessed via the [`VerificationResult`]
    /// result return type.
    ///
    /// # Warning
    ///
    /// Signature verification is currently always failing since no sender keys
    /// are fetched yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the encrypted attachment fetching or decryption fails.
    /// Signature verification failures are not returned as errors.
    #[tracing::instrument(skip_all, fields(id=?attachment_id))]
    pub async fn get_attachment(
        ctx: &MailUserContext,
        attachment_id: LocalAttachmentId,
    ) -> MailContextResult<DecryptedAttachment> {
        let attachment = Self::sync(ctx, attachment_id)
            .await
            .inspect_err(|e| error!("Failed to sync attachment: {e:?}"))?;
        let mut tether = ctx.user_stash().connection();
        let data_path = attachment
            .content_path(ctx, &mut tether)
            .await
            .inspect_err(|e| error!("Failed to get attachment path: {e:?}"))?;
        Ok(DecryptedAttachment {
            attachment_metadata: AttachmentMetadata {
                local_id: Some(attachment_id),
                attachment_type: attachment.attachment_type,
                disposition: attachment.disposition,
                mime_type: attachment.mime_type,
                filename: attachment.filename,
                size: attachment.size,
            },
            data_path,
        })
    }

    /// Starts a transaction, returns the fs path to the attachment and updates hit/atime metadata
    async fn path_from_cache_and_update_metadata_atomic(
        id: LocalAttachmentId,
        tx: &mut impl RunTransaction,
    ) -> MailContextResult<Option<PathBuf>> {
        let res = tx
            .run_tx(async |tx| {
                if let Some(path) = Self::path_from_cache_and_update_metadata(id, tx).await? {
                    return Ok(Some(path));
                };
                Ok(None)
            })
            .await
            .map_err(MailContextError::IntoTransactionError);

        let Ok(Some(path)) = res else { return res };

        match fs::try_exists(&path).await {
            Ok(true) => Ok(Some(path)),
            _ => {
                warn!("File no longer exists in fs");
                Ok(None)
            }
        }
    }

    /// Returns a fs path to an attachment in the filesystem.
    /// Also updates hit/atime metadata
    #[tracing::instrument(skip(tx))]
    pub async fn path_from_cache_and_update_metadata(
        id: LocalAttachmentId,
        tx: &Bond<'_>,
    ) -> Result<Option<PathBuf>, StashError> {
        let path = tx
            .query_value::<_, String>(
                indoc! {
                    "
                    SELECT path as value FROM attachment_cache
                    WHERE attachment_id = ?1;
                    "
                },
                params![id],
            )
            .await;

        match path {
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => Ok(None),
            Ok(path) => {
                let Ok(true) = fs::try_exists(&path).await else {
                    warn!("File was removed externally");

                    tx.execute(
                        indoc! {
                            "DELETE FROM attachment_cache
                             WHERE attachment_id = ?"
                        },
                        params![id],
                    )
                    .await?;
                    return Ok(None);
                };
                tx.execute(
                    indoc! { "
                       UPDATE attachment_cache
                       SET
                           atime = unixepoch('now'),
                           hit_count = hit_count + 1
                       WHERE attachment_id = ?1;
"},
                    params![id],
                )
                .await?;
                Ok(Some(PathBuf::from(path)))
            }
            Err(e) => Err(e),
        }
    }

    /// Creates the attachment in the attachment_cache table and stores it in the disk
    /// Returns the path as a String.
    #[tracing::instrument(skip(ctx, data, bond))]
    pub async fn store_in_cache(
        ctx: &MailUserContext,
        name: &str,
        id: LocalAttachmentId,
        data: Vec<u8>,
        bond: &Bond<'_>,
    ) -> MailContextResult<PathBuf> {
        tracing::debug!("Storing attachment in cache");
        let (path, path_string) = Self::attachment_cache_file_path(ctx, id, name).await?;

        let data_len = data.len();
        safe_write_async(&path, data).await?;
        bond.execute(
            indoc! {
            "INSERT INTO attachment_cache (attachment_id, path, size)
            VALUES (?, ?, ?)",
                    },
            params![id, path_string, data_len],
        )
        .await?;
        // Execute the cleanup routine in the background.
        Self::cleanup_cache(ctx).await;
        Ok(path)
    }

    /// Creates the attachment in the attachment_cache table and copies the
    /// contents from the given `attachment_path`.
    ///
    /// Returns the path as a String.
    ///
    /// # Errors
    ///
    /// Returns error if the copy of the data or the db query failed.
    #[tracing::instrument(skip(ctx, bond))]
    pub async fn copy_attachment_to_cache(
        ctx: &MailUserContext,
        name: &str,
        id: LocalAttachmentId,
        attachment_path: &Path,
        bond: &Bond<'_>,
    ) -> MailContextResult<PathBuf> {
        let metadata = tokio::fs::metadata(attachment_path).await?;
        let data_size = metadata.size();

        let (path, path_string) = Self::attachment_cache_file_path(ctx, id, name).await?;

        tokio::fs::copy(attachment_path, &path)
            .await
            .inspect_err(|e| error!("Failed to copy attachment: {e:?}"))?;

        bond.execute(
            indoc! {
            "INSERT INTO attachment_cache (attachment_id, path, size)
            VALUES (?, ?, ?)",
                    },
            params![id, path_string, data_size],
        )
        .await?;
        Ok(path)
    }

    async fn attachment_cache_file_path(
        ctx: &MailUserContext,
        attachment_id: LocalAttachmentId,
        filename: &str,
    ) -> Result<(PathBuf, String), MailContextError> {
        // We will write the attachment to
        // {CACHE_PATH}/attachments/{id}/{name}
        // The reason for this scheme is twofold:
        // - The clients require that the name of the attachment be the name
        // - Two different attachments might share the same name.
        let mut path = ctx.mail_context().attachments_cache_path();
        path.push(format!("{attachment_id}"));
        tokio::fs::create_dir_all(&path).await?;
        path.push(sanitize_filename(filename));

        let path_string = path
            .clone()
            .into_os_string()
            .into_string()
            // This is infallible since all pieces exist as a string at some point.
            .map_err(MailContextError::InvalidUtf8AttachmentPath)?;

        Ok((path, path_string))
    }

    /// Fetches and decrypts an attachment from the API.
    pub async fn fetch_data(&self, ctx: &MailUserContext) -> MailContextResult<Vec<u8>> {
        let attachment_id = self.id();
        let pgp = new_pgp_provider();

        let remote_attachment_id = match &self.attachment_type {
            AttachmentType::Remote(Some(id)) => id,
            AttachmentType::Remote(None) => {
                return Err(MailContextError::CalledFetchedAttachmentLocalAttachment);
            }
            AttachmentType::Pgp => {
                return Err(MailContextError::CalledFetchedAttachmentOnPgp);
            }
        };

        tracing::info!("Fetching {remote_attachment_id:?} from server");
        let encrypted_content =
            Attachment::fetch_content(remote_attachment_id.clone(), ctx.session())
                .await
                .map_err(|e| {
                    error!("Failed to fetch attachment ({attachment_id:?}) from API: {e:?}");
                    e
                })?;

        let (decrypted_content, _verification_result) = self
            .decrypt_content(ctx, &pgp, encrypted_content.as_ref())
            .await
            // There is not much we can do at this point, and we need to convert from a
            // MailboxError
            .context("Failed to decrypt attachment")
            .map_err(|e| {
                error!("{e:?}");
                MailContextError::Crypto
            })?;

        Ok(decrypted_content)
    }

    /// Sync attachment metadata
    pub async fn sync(
        ctx: &MailUserContext,
        attachment_id: LocalAttachmentId,
    ) -> MailContextResult<Attachment> {
        let mut conn = ctx.user_stash().connection();
        let mut attachment = Attachment::load(attachment_id, &conn)
            .await
            .inspect_err(|e| {
                error!("Failed to load attachment({attachment_id:?}) from DB: {e:?})")
            })?
            .ok_or(AppError::AttachmentMissing(attachment_id))?;
        // First check if the metadata is complete for decryption.
        if !attachment.is_pgp_attachment() && !attachment.has_complete_metadata() {
            attachment
                .sync_complete_metadata(ctx.session(), &mut conn)
                .await
                .inspect_err(|e| {
                    error!("Failed to sync attachment({attachment_id:?}) metadata: {e:?})")
                })
                .map_err(MailContextError::from)?;
            // Load the complete attachment metadata.
            attachment = Attachment::load(attachment_id, &conn)
                .await?
                .ok_or(AppError::AttachmentMissing(attachment_id))?;
        }
        Ok(attachment)
    }

    /// Decrypt attachment content
    pub async fn decrypt_content<P>(
        &self,
        ctx: &MailUserContext,
        pgp: &P,
        data: impl Read,
    ) -> MailContextResult<(Vec<u8>, VerificationResult)>
    where
        P: PGPProviderSync,
    {
        // Can't decrypt with the remote address id.
        let Some(remote_address_id) = &self.remote_address_id else {
            return Err(AppError::AttachmentHasNoAddressId(self.id()).into());
        };

        // Sanity check that the key packets are set, there is an expect() in the decryption
        // code that can trigger application crashes.
        if self.key_packets.is_none() {
            return Err(AppError::AttachmentMissingKeyPackets(self.id()).into());
        }

        let mut result_buffer: Vec<u8> =
            Vec::with_capacity(self.size.try_into().unwrap_or_default());

        let tether = ctx.user_stash().connection();

        let address_keys = ctx
            .unlocked_address_keys(pgp, &tether, remote_address_id)
            .await?;

        // TODO: Load the sender verification keys for correct signature verification.
        let verification_keys: Vec<<P as PGPProvider>::PublicKey> = Vec::new();

        let mut decrypting_reader = self
            .decrypt_from_reader(pgp, address_keys.as_ref(), &verification_keys, data)
            .map_err(AppError::AttachmentDecryption)?;

        std::io::copy(&mut decrypting_reader, &mut result_buffer)
            .map_err(|e| AppError::AttachmentDecryptionIO(e.to_string()))?;

        let signature_verification = decrypting_reader.verification_result();

        Ok((result_buffer, signature_verification))
    }

    /// Based on some heuristics and assumptions it will try to delete the attachments that are less
    /// likely to be used.
    ///
    /// Each message's attachment have an utility score.
    /// This score can be thought as the size of an attachment divided by the probability P of it
    /// being accessed.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_possible_wrap)]
    #[tracing::instrument(skip_all)]
    pub async fn do_cleanup_cache(ctx: &MailUserContext) -> anyhow::Result<()> {
        // First let's check whether we should run. We run on two conditions:
        // 1. If the cache is too big
        // 2. If an attachment is scheduled for deletion (the attachment row has been deleted but not in the fs)
        let mut tether = ctx.user_stash().connection();
        let current_size = tether
            .query_value::<_, u64>(
                "SELECT IFNULL(SUM(size),0) AS value FROM attachment_cache",
                vec![],
            )
            .await
            .context("Error getting total cache utilization")?;

        let max_size = ctx.mail_context().attachment_cache_size;
        if current_size < max_size {
            trace!("Not deleting attachments from cache yet.");
            return Ok(());
        }
        info!("Cache is too large, trying to delete attachments...");

        let all_cache_items = AttachmentCacheMetadata::find("", vec![], &tether).await?;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("Time went backwards")?;

        let mut atts = vec![];
        for at_cache in all_cache_items {
            let Some(attachment) = Attachment::find_by_id(at_cache.attachment_id, &tether).await?
            else {
                // Garbage collect deleted attachments.
                atts.push((0.0, at_cache));
                continue;
            };
            // Some attachments have no messages assigned yet when creating drafts so it's fine to skip over them.
            if let Some(id) = attachment.local_message_id {
                if let Some(message) = Message::find_by_id(id, &tether).await? {
                    let utility = get_utility(&at_cache, &attachment, &message, now);
                    atts.push((utility, at_cache));
                } else {
                    error!(
                        "Bug: attachment.local_message_id doesn't have a message. This should be impossible because of the sql constraint."
                    );
                }
            }
        }

        atts.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        // atts.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).expect("NaN"));

        // We over delete a bit for good measure so that this doesn't have to run each time.

        // How much we overdelete each time this runs
        let over_factor = 0.1;
        let bytes_to_delete =
            (current_size as i64 - max_size as i64) + (max_size as i64 * over_factor as i64);

        let mut ids: Vec<Box<dyn ToSql + Send>> = vec![];
        let mut files = vec![];
        for (_, at) in attachments_to_delete(atts, bytes_to_delete) {
            files.push(at.path);
            debug!("Deleting {:?}", at.attachment_id);
            ids.push(Box::new(at.attachment_id));
        }

        if ids.is_empty() {
            info!("No attachments to delete");
            return Ok(());
        }
        info!("Deleting {} attachments", ids.len());

        match ctx.user_context().delete_files_safe(files) {
            Err(DeleteFilesSafeError::Failed(e)) => {
                // This will almost never happen in practice. This means that for whatever reason a
                // file move failed (?) of a file that hasn't been touched for a long time
                // (otherwise the atime would have been modified)
                error!(
                    "Could not move some of the files to the deletion dir. There's nothing we can do. {e:?}"
                );
            }
            Err(DeleteFilesSafeError::Moved(e)) => {
                error!("Could not delete files now, but will delete them later. {e:?}");
            }
            Ok(_) => (),
        }

        let query = format!(
            "DELETE FROM attachment_cache WHERE attachment_id IN ({})",
            placeholders_n(ids.len())
        );
        tether.tx(async |tx| tx.execute(query, ids).await).await?;

        Ok(())
    }

    /// This function ensures that this is called at most once concurrently, and spawns the
    /// cleanup routine in the background if it's not being currently executed.
    pub async fn cleanup_cache(ctx: &MailUserContext) {
        if ctx.origin() != Origin::App {
            return;
        };

        let state = ctx.attachment_cache_state();

        // TODO: Possibly run this in a background task instead of once-per.
        pub struct G(Arc<AtomicBool>);
        impl Drop for G {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }

        let is_executing = state.is_cleanup_running().clone();
        if is_executing.swap(true, Ordering::Acquire) {
            debug!("Cleanup routine already running");
            return;
        }
        let ctx_2 = ctx.as_arc();
        ctx.spawn(async move {
            let _g = G(is_executing);
            if let Err(e) = Self::do_cleanup_cache(&ctx_2).await {
                error!("Error cleaning up attachments: {e}");
            }
        });
    }

    pub fn is_pgp_attachment(&self) -> bool {
        matches!(self.attachment_type, AttachmentType::Pgp)
    }
}

/// Returns the probability that an attachment will be ever used, depending on when it was
/// last accessed.
fn atime_factor(diff: Duration) -> f64 {
    // Let's say that each day that passes since it was accessed makes it 5% less likely to be used.
    // This decays exponentially as the probability compounds.
    //
    // To make this more useful let's compound in seconds.
    // can't pow yet in consts so
    // (1 / 0.95) ^ 1 / (24 * 60 * 60)
    1.0 / 1.0000005936725649_f64.powi(
        diff.as_secs().try_into().unwrap_or(i32::MAX), // More than 68 years elapsed? :P
    )
}

// Precision loss is fine as we don't care about the real value, just the relative ordering.
#[allow(clippy::cast_precision_loss)]
fn get_utility(
    at_cache: &AttachmentCacheMetadata,
    attachment: &Attachment,
    message: &Message,
    now: Duration,
) -> f64 {
    // -------- CONSTANTS --------
    // All of these numbers are completely arbitrary.

    // Inline attachments are way more likely to be used as normal attachments
    let disp_inline = 1.0;
    let disp_attachment = 0.2;

    // How much importance we give to how often an attachment was accessed.
    // Right now, an attachment that is opened 2 times will be 3x more likely to be used.
    let hit_count_ratio = 1.0;

    // Spam and trash could be accounted for. They aren't for now but they could in the future just
    // by changing these.
    let spam = 1.0;
    let trash = 1.0;

    // Let's say that attachments of messages that have been starred are 10x more likely to be
    // used as normal
    let starred = 10.0;

    // It's less likely that the user will use inline attachments if it has the setting off.
    // Currently unused.
    let _disp_inline_disabled = 0.8;

    // -------- CODE --------

    // Prioritize deleting bigger attachments: bigger = less useful.
    let mut utility = 1.0 / at_cache.size as f64;

    // If for whatever reason atime > now atime_factor will return 1.0
    let elapsed = now.saturating_sub(Duration::from_secs(at_cache.atime));
    utility *= atime_factor(elapsed);
    utility *= (at_cache.hit_count as f64 + 1.0) * hit_count_ratio;

    if message.is_draft()
        || message.label_ids.contains(&LabelId::outbox())
        || !matches!(attachment.attachment_type, AttachmentType::Remote(Some(_)))
    {
        // Either PGP attachment or draft.
        // Unsent draft attachments must be retained
        // PGP attachments must be retained because we can't re-request them.
        return f64::INFINITY;
    }

    if at_cache.size == 0 {
        // 0 sized attachments are useless
        return 0.0;
    }

    match attachment.disposition {
        Disposition::Attachment => {
            utility *= disp_attachment;
        }
        Disposition::Inline => {
            utility *= disp_inline;
        }
    }

    if let Some(ExclusiveLocation::System { name, .. }) = &message.exclusive_location {
        if *name == SystemLabel::Trash {
            utility *= trash;
        } else if *name == SystemLabel::Spam {
            utility *= spam;
        }
    }

    if message.is_starred() {
        utility *= starred;
    }

    if utility.is_nan() {
        error!("NaN utility for attachment {}", at_cache.attachment_id);
        return 0.0;
    }
    utility
}

#[allow(clippy::cast_possible_wrap)]
fn attachments_to_delete(
    mut atts: Vec<(f64, AttachmentCacheMetadata)>,
    mut bytes_to_delete: i64,
) -> impl Iterator<Item = (f64, AttachmentCacheMetadata)> {
    atts.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).expect("Utility was NaN"));
    atts.into_iter().take_while(move |(utility, at)| {
        // If we've deleted enough
        // and all remaining attachments are not useless (utility > 0, this is guaranteed
        // because they are sorted)
        if bytes_to_delete < 0 && *utility > 0.0 {
            return false;
        }

        if *utility == f64::INFINITY {
            return false; // From here on we can't delete anything else, even we're low on space.
        }

        bytes_to_delete -= at.size as i64;
        true
    })
}

#[cfg(test)]
mod test {
    use std::sync::atomic::AtomicU64;

    use itertools::Itertools as _;
    use proton_core_api::services::proton::LabelId;

    use crate::datatypes::SystemLabelId as _;

    use super::*;
    #[derive(Default)]
    struct UsedVariables {
        /// Unique identifier that will get used to identify the item. It will act as the path.
        attachment_name: &'static str,
        disposition: Disposition,
        hit_count: u64,
        att_type: AttachmentType,
        spam: bool,
        trash: bool,
        starred: bool,
        /// Last time it was accessed
        atime: Duration,
        size: u64,
    }

    impl UsedVariables {
        fn unpack(self, now: Duration) -> (AttachmentCacheMetadata, Attachment, Message) {
            let at_cache = AttachmentCacheMetadata {
                attachment_id: 0.into(),
                atime: (now - self.atime).as_secs(),
                hit_count: self.hit_count,
                size: self.size,
                path: String::from(self.attachment_name),
                ctime: Default::default(),
            };
            let attachment = Attachment {
                disposition: self.disposition,
                attachment_type: self.att_type,
                filename: self.attachment_name.into(),
                ..Default::default()
            };

            let mut message = Message::test_default();

            if self.starred {
                message.label_ids.push(LabelId::starred());
            }
            if self.trash {
                message.exclusive_location = Some(ExclusiveLocation::System {
                    name: SystemLabel::Trash,
                    local_id: 0.into(),
                });
            }
            if self.spam {
                if self.trash {
                    panic!("Conflicting trash and spam fields")
                }

                message.exclusive_location = Some(ExclusiveLocation::System {
                    name: SystemLabel::Trash,
                    local_id: 1.into(),
                });
            }
            (at_cache, attachment, message)
        }
    }

    fn default() -> UsedVariables {
        static UNIQUE: AtomicU64 = AtomicU64::new(0);
        let remote_id = UNIQUE.fetch_add(1, Ordering::Relaxed).to_string();

        UsedVariables {
            att_type: AttachmentType::Remote(Some(remote_id.into())),
            ..Default::default()
        }
    }
    fn kb(bytes: u64) -> u64 {
        bytes * 1024
    }

    fn test_order(vars: impl IntoIterator<Item = UsedVariables>) -> String {
        test_order_with_bytes(vars, i64::MAX)
    }

    /// This function will emulate deleting everything, just to assert the order.
    fn test_order_with_bytes(
        vars: impl IntoIterator<Item = UsedVariables>,
        bytes_to_delete: i64,
    ) -> String {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Time went backwards");
        let atts = vars
            .into_iter()
            .map(|var| var.unpack(now))
            .map(|(at_cache, attachment, message)| {
                let utility = get_utility(&at_cache, &attachment, &message, now);
                (utility, at_cache)
            })
            .collect_vec();

        attachments_to_delete(atts, bytes_to_delete)
            .map(|(utility, att)| {
                if utility == f64::INFINITY {
                    format!("name: {}, INFINITY", att.path)
                } else if utility == 0.0 {
                    format!("name: {}, utility 0", att.path)
                } else {
                    format!("name: {}, anti-utility {:.0}", att.path, 1.0 / utility)
                }
            })
            .join("\n")
    }

    #[test]
    fn smaller_is_better() {
        let vars = [
            UsedVariables {
                attachment_name: "small",
                size: kb(5),
                ..default()
            },
            UsedVariables {
                attachment_name: "medium",
                size: kb(10),
                ..default()
            },
            UsedVariables {
                attachment_name: "large",
                size: kb(20),
                ..default()
            },
        ];

        insta::assert_snapshot!(test_order(vars));
    }

    fn days(days: u64) -> Duration {
        Duration::from_secs(60 * 60 * 24 * days)
    }

    #[test]
    fn trash_and_spam_are_catastrophic() {
        let vars = [
            UsedVariables {
                attachment_name: "very big but not trash or spam",
                size: kb(100),
                ..default()
            },
            UsedVariables {
                attachment_name: "spam",
                spam: true,
                size: kb(15),
                ..default()
            },
            UsedVariables {
                attachment_name: "trash",
                trash: true,
                size: kb(15),
                ..default()
            },
        ];
        insta::assert_snapshot!(test_order(vars));
    }

    #[test]
    fn older_attachments_have_exponentially_worse_score() {
        let vars = [
            UsedVariables {
                attachment_name: "big but new",
                size: kb(100_000),
                atime: Duration::from_secs(1),
                ..default()
            },
            UsedVariables {
                attachment_name: "smol but ol'",
                size: kb(50),
                atime: days(365 * 5),
                ..default()
            },
            UsedVariables {
                attachment_name: "happy medium",
                size: kb(150),
                atime: days(500),
                ..default()
            },
        ];
        insta::assert_snapshot!(test_order(vars));
    }

    #[test]
    fn hit_count_influence() {
        let vars = [
            UsedVariables {
                attachment_name: "high hits large",
                size: kb(100),
                hit_count: 10,
                ..default()
            },
            UsedVariables {
                attachment_name: "low hits small",
                size: kb(50),
                hit_count: 1,
                ..default()
            },
            UsedVariables {
                attachment_name: "no hits medium",
                size: kb(75),
                hit_count: 0,
                ..default()
            },
        ];
        insta::assert_snapshot!(test_order(vars));
    }

    #[test]
    fn inline_vs_attachment_disposition() {
        let vars = [
            UsedVariables {
                attachment_name: "inline small",
                size: kb(50),
                disposition: Disposition::Inline,
                ..default()
            },
            UsedVariables {
                attachment_name: "inline large",
                size: kb(100),
                disposition: Disposition::Inline,
                ..default()
            },
            UsedVariables {
                attachment_name: "attachment small",
                size: kb(50),
                disposition: Disposition::Attachment,
                ..default()
            },
            UsedVariables {
                attachment_name: "attachment large",
                size: kb(100),
                disposition: Disposition::Attachment,
                ..default()
            },
        ];
        insta::assert_snapshot!(test_order(vars));
    }

    #[test]
    fn starred_message_boost() {
        let vars = [
            UsedVariables {
                attachment_name: "starred small",
                size: kb(50),
                starred: true,
                ..default()
            },
            UsedVariables {
                attachment_name: "starred large",
                size: kb(200),
                starred: true,
                ..default()
            },
            UsedVariables {
                attachment_name: "normal small",
                size: kb(50),
                ..default()
            },
            UsedVariables {
                attachment_name: "normal large",
                size: kb(200),
                ..default()
            },
        ];
        insta::assert_snapshot!(test_order(vars));
    }

    #[test]
    fn pgp_attachment_priority() {
        let vars = [
            UsedVariables {
                attachment_name: "pgp",
                size: kb(150),
                att_type: AttachmentType::Pgp,
                ..default()
            },
            UsedVariables {
                attachment_name: "regular",
                size: kb(150),
                ..default()
            },
        ];
        insta::assert_snapshot!(test_order(vars));
    }

    #[test]
    fn zero_sized_deleted() {
        let vars = [
            UsedVariables {
                attachment_name: "zero_size",
                size: 0,
                ..default()
            },
            UsedVariables {
                attachment_name: "tiny",
                size: 1,
                ..default()
            },
        ];
        insta::assert_snapshot!(test_order(vars));
    }

    #[test]
    fn partial_deletion_scenario() {
        // This tests  the scenario where only some of the attachments get deleted, when their sum
        // exceeds 250_000
        let vars = [
            UsedVariables {
                attachment_name: "first_delete_me",
                size: kb(100),
                ..default()
            },
            UsedVariables {
                attachment_name: "second_delete_me",
                size: kb(200),
                ..default()
            },
            UsedVariables {
                attachment_name: "keep_me",
                size: kb(50),
                ..default()
            },
        ];
        insta::assert_snapshot!(test_order_with_bytes(vars, 250_000));
    }

    fn comprehensive() -> impl Iterator<Item = UsedVariables> {
        [
            // Zero-sized attachments should always be preserved regardless of other factors
            UsedVariables {
                attachment_name: "zero_sized_in_trash",
                size: 0,
                trash: true,
                atime: days(365),
                hit_count: 0,
                ..default()
            },
            // Spam and trash have catastrophic impact on priority
            UsedVariables {
                attachment_name: "spam_tiny_new",
                size: kb(5),
                spam: true,
                atime: Duration::from_secs(3600), // 1 hour
                hit_count: 3,
                ..default()
            },
            UsedVariables {
                attachment_name: "trash_tiny_new",
                size: kb(5),
                trash: true,
                atime: Duration::from_secs(3600), // 1 hour
                hit_count: 3,
                ..default()
            },
            // Age significantly impacts priority
            UsedVariables {
                attachment_name: "ancient_tiny",
                size: kb(10),
                atime: days(720), // 2 years
                hit_count: 1,
                ..default()
            },
            UsedVariables {
                attachment_name: "recent_large",
                size: kb(200),
                atime: Duration::from_secs(3600 * 24), // 1 day
                hit_count: 1,
                ..default()
            },
            // Hit count provides significant protection
            UsedVariables {
                attachment_name: "frequently_accessed_medium",
                size: kb(50),
                hit_count: 20,
                atime: days(30),
                ..default()
            },
            UsedVariables {
                attachment_name: "rarely_accessed_small",
                size: kb(20),
                hit_count: 0,
                atime: days(30),
                ..default()
            },
            // Starred messages get significant protection
            UsedVariables {
                attachment_name: "starred_large",
                size: kb(100),
                starred: true,
                hit_count: 1,
                atime: days(14),
                ..default()
            },
            UsedVariables {
                attachment_name: "unstarred_medium",
                size: kb(40),
                starred: false,
                hit_count: 1,
                atime: days(14),
                ..default()
            },
            // PGP attachments are prioritized
            UsedVariables {
                attachment_name: "pgp_large",
                size: kb(80),
                att_type: AttachmentType::Pgp,
                hit_count: 1,
                atime: days(7),
                ..default()
            },
            UsedVariables {
                attachment_name: "normal_small",
                size: kb(30),
                hit_count: 1,
                atime: days(7),
                ..default()
            },
            // Disposition differences
            UsedVariables {
                attachment_name: "inline_medium",
                size: kb(60),
                disposition: Disposition::Inline,
                hit_count: 1,
                atime: days(5),
                ..default()
            },
            UsedVariables {
                attachment_name: "attachment_medium",
                size: kb(60),
                disposition: Disposition::Attachment,
                hit_count: 1,
                atime: days(5),
                ..default()
            },
            // Complex combinations
            UsedVariables {
                attachment_name: "starred_trash_large", // competing factors
                size: kb(90),
                starred: true,
                trash: true,
                hit_count: 2,
                atime: days(2),
                ..default()
            },
            UsedVariables {
                attachment_name: "pgp_old_but_frequent", // competing factors
                size: kb(70),
                att_type: AttachmentType::Pgp,
                hit_count: 15,
                atime: days(180),
                ..default()
            },
            UsedVariables {
                attachment_name: "tiny_inline_ancient_but_starred", // competing factors
                size: kb(5),
                disposition: Disposition::Inline,
                atime: days(500),
                starred: true,
                hit_count: 1,
                ..default()
            },
            // Additional edge cases
            UsedVariables {
                attachment_name: "unsent_draft_large",
                size: kb(150),
                att_type: AttachmentType::Remote(None),
                atime: days(60),
                hit_count: 0,
                ..default()
            },
            UsedVariables {
                attachment_name: "fresh_but_huge",
                size: kb(500),
                atime: Duration::from_secs(60), // 1 minute
                hit_count: 0,
                ..default()
            },
            UsedVariables {
                attachment_name: "perfect_storm", // everything negative
                size: kb(300),
                trash: true,
                atime: days(365),
                hit_count: 0,
                disposition: Disposition::Attachment,
                ..default()
            },
            UsedVariables {
                attachment_name: "golden_child", // everything positive
                size: kb(30),
                starred: true,
                disposition: Disposition::Inline,
                hit_count: 10,
                atime: Duration::from_secs(300), // 5 minutes
                ..default()
            },
        ]
        .into_iter()
    }

    #[test]
    fn comprehensive_priority_test() {
        // Test with unlimited bytes to delete to see full priority order
        insta::assert_snapshot!(test_order(comprehensive()));
    }

    #[test]
    fn atime_factor_edge_cases() {
        assert_eq!(atime_factor(Duration::from_secs(0)), 1.0);
        assert_eq!(atime_factor(Duration::from_secs(u64::MAX)), 0.0);
    }

    #[test]
    fn test_atime_factor() {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(output, "--- SECS  ---").unwrap();
        let mut test_secs = |secs: u64| {
            let factor = atime_factor(Duration::from_secs(secs));
            writeln!(output, "{secs:?}: {factor:.5e}").unwrap();
        };
        test_secs(0);
        test_secs(1);
        test_secs(10);
        test_secs(100);
        test_secs(500);

        writeln!(output, "--- HOURS ---").unwrap();
        let mut test_hour = |hours: u64| {
            let factor = atime_factor(Duration::from_secs(hours * 3600));
            writeln!(output, "{hours}h: {factor:.5e}").unwrap();
        };

        test_hour(1);
        test_hour(4);
        test_hour(12);

        writeln!(output, "--- DAYS  ---").unwrap();
        let mut test_day = |day: u64| {
            let factor = atime_factor(days(day));
            writeln!(output, "{day}d: {factor:.5e}").unwrap();
        };
        test_day(1);
        test_day(2);
        test_day(10);
        test_day(30);
        test_day(90);
        test_day(180);

        writeln!(output, "--- YEARS ---").unwrap();
        let mut test_year = |year: u64| {
            let factor = atime_factor(days(year * 365));
            writeln!(output, "{year}y: {factor:.5e}").unwrap();
        };
        test_year(1);
        test_year(2);
        test_year(3);
        test_year(4);
        test_year(5);
        test_year(7);
        test_year(10);
        test_year(10);
        test_year(15);
        test_year(20);

        insta::assert_snapshot!(output);
    }
}
