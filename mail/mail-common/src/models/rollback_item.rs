use crate::MailContextError;
use crate::datatypes::RollbackItemType;
use crate::models::{Conversation, Message};
use futures::stream::{FuturesUnordered, StreamExt};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::services::proton::{LabelId, ProtonIdMarker};
use proton_core_api::session::{CoreSession, Session};
use proton_core_common::models::Label;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::MessageMetadata;
use proton_mail_api::services::proton::requests::{GetConversationsOptions, GetMessagesOptions};
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, RunTransaction, StashError, Tether};
use tracing::{debug, error};

#[cfg(test)]
#[path = "../tests/models/rollback_item.rs"]
mod tests;

/// A record of an action that was rolled back.
///
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("rollback_actions")]
pub struct RollbackItem {
    #[IdField]
    pub remote_id: String,

    #[DbField]
    pub item_type: RollbackItemType,

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl RollbackItem {
    pub fn new(remote_id: String, item_type: RollbackItemType) -> Self {
        Self {
            remote_id,
            item_type,
            row_id: Default::default(),
        }
    }

    /// Save or update a RollbackItem.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// # Errors
    ///
    /// When the query fails.
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        let None = RollbackItem::find_first(
            "WHERE remote_id=? AND item_type=?",
            params![self.remote_id.clone(), self.item_type],
            bond,
        )
        .await?
        else {
            return Ok(());
        };

        <Self as Model>::save(self, bond).await
    }

    /// Synchronize all rollback items with remote counterparts.
    /// This method will be invoked by external workers to keep the local
    /// data in sync with the API. In theory it should not be necessary to
    /// do so, but in practice, and especially for alpha release, it is useful
    /// to have a way to recover from malfunctions.
    ///
    /// ## Parameters
    /// * `session`   - The API client to use for syncing.
    /// * `tx_runner` - Transaction runner implementor.
    /// * `batch`     - The number of items to sync in a single batch.
    ///
    /// ## Errors
    ///
    /// This method will return an error if any of the API requests fail.
    /// It will also return an error if any of the local database operations fail.
    /// Method cleans up the local database by deleting the records that have
    /// been synced, so double syncing should never happen.
    ///
    ///
    pub async fn sync_all<I>(
        session: &Session,
        tx: &mut impl RunTransaction,
        batch: I,
    ) -> Result<(), MailContextError>
    where
        I: Into<Option<usize>> + Copy,
    {
        Self::sync_labels(session, tx, batch).await?;
        Self::sync_messages(session, tx, batch).await?;
        Self::sync_conversations(session, tx, batch).await?;

        Ok(())
    }

    /// Synchronize all labels with remote counterparts.
    ///
    /// ## Parameters & Errors
    ///
    /// Look at the documentation of the `sync_all` method.
    ///
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn sync_labels<I>(
        session: &Session,
        tx: &mut impl RunTransaction,
        batch: I,
    ) -> Result<(), MailContextError>
    where
        I: Into<Option<usize>>,
    {
        Self::sync_items_impl::<LabelRollbackHandler, _>(session, tx, batch.into()).await
    }

    /// Synchronize all messages with remote counterparts.
    ///
    /// ## Parameters & Errors
    ///
    /// Look at the documentation of the `sync_all` method.
    ///
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn sync_messages<I>(
        session: &Session,
        tx: &mut impl RunTransaction,
        batch: I,
    ) -> Result<(), MailContextError>
    where
        I: Into<Option<usize>>,
    {
        Self::sync_items_impl::<MessageRollbackHandler, _>(session, tx, batch.into()).await
    }

    /// Synchronize all conversations with remote counterparts.
    ///
    /// ## Parameters & Errors
    ///
    /// Look at the documentation of the `sync_all` method.
    ///
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn sync_conversations<I>(
        session: &Session,
        tx: &mut impl RunTransaction,
        batch: I,
    ) -> Result<(), MailContextError>
    where
        I: Into<Option<usize>>,
    {
        Self::sync_items_impl::<ConversationRollbackHandler, _>(session, tx, batch.into()).await
    }

    /// This helper method is used to find all rollback items of a specific kind.
    ///
    /// ## Parameters
    ///
    /// * `kind` - The kind of the rollback item to find.
    /// * `interface` - The interface to use for the database operations.
    ///
    /// ## Errors
    ///
    /// This method will return an error if the database operation fails.
    ///
    #[cfg(test)]
    async fn find_by_kind(
        kind: RollbackItemType,
        tether: &Tether,
    ) -> Result<Vec<RollbackItem>, StashError> {
        RollbackItem::find("WHERE item_type = ?", params![kind], tether).await
    }

    /// This helper method is used to find all rollback items of a specific kind.
    ///
    /// ## Parameters
    ///
    /// * `kind` - The kind of the rollback item to find.
    /// * `interface` - The interface to use for the database operations.
    ///
    /// ## Errors
    ///
    /// This method will return an error if the database operation fails.
    ///
    async fn find_remote_ids_by_kind(
        kind: RollbackItemType,
        tether: &Tether,
    ) -> Result<Vec<String>, StashError> {
        tether
            .query_values::<_, String>(
                format!(
                    "SELECT remote_id AS value FROM {} WHERE item_type = ?",
                    Self::table_name()
                ),
                params![kind],
            )
            .await
    }

    /// This helper method is used to delete rollback item of a specific kind & remote_id.
    ///
    /// ## Parameters
    ///
    /// * `remote_id` - The remote ID of the rollback item to delete.
    /// * `kind` - The kind of the rollback item to delete.
    /// * `tether` - The interface to use for the database operations.
    ///
    /// ## Errors
    ///
    /// This method will return an error if the database operation fails.
    ///
    async fn delete_by_rid_and_kind(
        remote_id: Option<String>,
        kind: RollbackItemType,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            format!(
                "DELETE FROM {} WHERE remote_id = ? AND item_type = ?",
                Self::table_name()
            ),
            params![remote_id, kind],
        )
        .await?;

        Ok(())
    }

    async fn sync_items_impl<H: RollbackHandler, T: RunTransaction>(
        session: &Session,
        tx_runner: &mut T,
        batch: Option<usize>,
    ) -> Result<(), MailContextError> {
        let items: Vec<H::RemoteId> =
            Self::find_remote_ids_by_kind(H::item_type(), tx_runner.tether())
                .await?
                .into_iter()
                .map(H::RemoteId::from)
                .collect();
        if items.is_empty() {
            // Nothing to sync.
            return Ok(());
        }
        debug!("Found {} items to sync", items.len());
        let batch = batch.unwrap_or(items.len() + 1);

        // Can't use itertools chunks as it is not send compatible.
        let batches = items
            .chunks(batch)
            .map(async |ids| Ok((ids, H::fetch_items(session, ids).await?)));

        let mut tasks = FuturesUnordered::from_iter(batches);

        while let Some(result) = tasks.next().await {
            let (ids, items) = result.inspect_err(|e: &ApiServiceError| {
                error!("Failed to fetch batch ({:?}): {e:?}", H::item_type());
            })?;
            tx_runner
                .run_tx(async |tx| {
                    H::store_items(items, tx).await.inspect_err(|e| {
                        error!("Failed to store items ({:?}): {e:?}", H::item_type());
                    })?;

                    for id in ids {
                        Self::delete_by_rid_and_kind(Some((*id).to_string()), H::item_type(), tx)
                            .await
                            .inspect_err(|e| {
                                error!(
                                    "Failed to delete rollback item {id}({:?}): {e:?}",
                                    H::item_type()
                                );
                            })?;
                    }
                    Ok(())
                })
                .await
                .map_err(MailContextError::Other)?;
        }
        debug!("Sync finished");
        Ok(())
    }
}

/// Defines behaviors for use with `RollbackItem::sync_items_impl`.
trait RollbackHandler: 'static + Send + Sync {
    type Item: 'static;
    type RemoteId: ProtonIdMarker + From<String> + std::fmt::Display;

    /// Return the respective [`RollbackItemType`] for this handler.
    fn item_type() -> RollbackItemType;

    /// Fetch the items with `remote_ids` from the server.
    ///
    /// # Errors
    ///
    /// Return error if the fetching failed.
    fn fetch_items(
        session: &Session,
        remote_ids: &[Self::RemoteId],
    ) -> impl Future<Output = Result<Vec<Self::Item>, ApiServiceError>> + Send;

    /// Convert and store the `items` in the local database.
    ///
    /// # Errors
    ///
    /// Return error if the conversion or the storing of the items failed.
    fn store_items(
        items: Vec<Self::Item>,
        tx: &Bond<'_>,
    ) -> impl Future<Output = Result<(), MailContextError>>;
}

struct MessageRollbackHandler {}

impl RollbackHandler for MessageRollbackHandler {
    type Item = MessageMetadata;
    type RemoteId = MessageId;
    fn item_type() -> RollbackItemType {
        RollbackItemType::Message
    }

    async fn fetch_items(
        session: &Session,
        remote_ids: &[Self::RemoteId],
    ) -> Result<Vec<Self::Item>, ApiServiceError> {
        let options = GetMessagesOptions {
            page: 0,
            page_size: remote_ids.len() as u64,
            ids: Some(remote_ids.to_vec()),
            ..Default::default()
        };
        Ok(session.api().get_messages(options).await?.messages)
    }

    async fn store_items(items: Vec<Self::Item>, tx: &Bond<'_>) -> Result<(), MailContextError> {
        for item in items {
            let mut message = Message::from_api_metadata(item, tx).await?;
            message.save(tx).await?;
        }
        Ok(())
    }
}

struct ConversationRollbackHandler {}

impl RollbackHandler for ConversationRollbackHandler {
    type Item = proton_mail_api::services::proton::response_data::Conversation;
    type RemoteId = ConversationId;
    fn item_type() -> RollbackItemType {
        RollbackItemType::Conversation
    }

    async fn fetch_items(
        session: &Session,
        remote_ids: &[Self::RemoteId],
    ) -> Result<Vec<Self::Item>, ApiServiceError> {
        let options = GetConversationsOptions {
            page: 0,
            page_size: remote_ids.len() as u64,
            ids: Some(remote_ids.to_vec()),
            ..Default::default()
        };
        Ok(session
            .api()
            .get_conversations(options)
            .await?
            .conversations)
    }

    async fn store_items(items: Vec<Self::Item>, tx: &Bond<'_>) -> Result<(), MailContextError> {
        for item in items {
            let mut message = Conversation::from(item);
            message.save(tx).await?;
        }
        Ok(())
    }
}

struct LabelRollbackHandler {}

impl RollbackHandler for LabelRollbackHandler {
    type Item = proton_core_api::services::proton::Label;
    type RemoteId = LabelId;
    fn item_type() -> RollbackItemType {
        RollbackItemType::Label
    }

    async fn fetch_items(
        session: &Session,
        remote_ids: &[Self::RemoteId],
    ) -> Result<Vec<Self::Item>, ApiServiceError> {
        Ok(session
            .api()
            .get_labels_by_ids(remote_ids.to_vec())
            .await?
            .labels)
    }

    async fn store_items(items: Vec<Self::Item>, tx: &Bond<'_>) -> Result<(), MailContextError> {
        for item in items {
            let mut label = Label::from(item);
            label.save(tx).await?;
        }
        Ok(())
    }
}

#[cfg(any(test, debug_assertions))]
mod test_utils {
    use super::*;

    impl<'a> From<&'a Label> for RollbackItem {
        fn from(label: &'a Label) -> Self {
            Self {
                remote_id: label.remote_id.clone().unwrap().into_inner(),
                item_type: RollbackItemType::Label,
                row_id: None,
            }
        }
    }

    impl From<Label> for RollbackItem {
        fn from(label: Label) -> Self {
            Self::from(&label)
        }
    }

    impl<'a> From<&'a Message> for RollbackItem {
        fn from(message: &'a Message) -> Self {
            Self {
                remote_id: message.remote_id.clone().unwrap().into_inner(),
                item_type: RollbackItemType::Message,
                row_id: None,
            }
        }
    }

    impl From<Message> for RollbackItem {
        fn from(message: Message) -> Self {
            Self::from(&message)
        }
    }

    impl<'a> From<&'a Conversation> for RollbackItem {
        fn from(conversation: &'a Conversation) -> Self {
            Self {
                remote_id: conversation.remote_id.clone().unwrap().into_inner(),
                item_type: RollbackItemType::Conversation,
                row_id: None,
            }
        }
    }

    impl From<Conversation> for RollbackItem {
        fn from(conversation: Conversation) -> Self {
            Self::from(&conversation)
        }
    }
}
