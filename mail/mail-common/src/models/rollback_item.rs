use crate::MailContextError;
use crate::datatypes::RollbackItemType;
use crate::datatypes::dependencies::DependencyFetcher;
use crate::models::{Conversation, Message, MessageSyncDecision};
use futures::stream::{FuturesOrdered, FuturesUnordered, StreamExt};
use proton_action_queue::queue::Queue;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::consts::Mail;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::services::proton::{LabelId, ProtonIdMarker};
use proton_core_api::session::Session;
use proton_core_common::models::Label;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::{GetConversationResponse, MessageMetadata};
use proton_mail_api::services::proton::requests::GetMessagesOptions;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, RunTransaction, StashError, Tether};
use std::fmt::Display;
use tracing::{debug, error, warn};

#[cfg(test)]
#[path = "../tests/models/rollback_item.rs"]
mod tests;

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("rollback_actions")]
pub struct RollbackItem {
    #[IdField]
    pub remote_id: String,

    #[DbField]
    pub item_type: RollbackItemType,
}

impl RollbackItem {
    pub fn new(remote_id: String, item_type: RollbackItemType) -> Self {
        Self {
            remote_id,
            item_type,
        }
    }

    pub async fn save_many(
        tx: &Bond<'_>,
        items: impl IntoIterator<Item = impl Display>,
        item_type: RollbackItemType,
    ) -> Result<(), StashError> {
        for item in items {
            Self::new(item.to_string(), item_type).save(tx).await?;
        }
        Ok(())
    }

    /// Save or update a RollbackItem.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is update correctly in the database.
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if RollbackItem::find_first(
            "WHERE remote_id=? AND item_type=?",
            params![self.remote_id.clone(), self.item_type],
            bond,
        )
        .await?
        .is_none()
        {
            // Only conditionally call Model::insert
            // Crucially this is not Model::save
            <Self as Model>::insert(self, bond).await?;
        } else {
            // We can skip the insert since it's already there.
            // An update would do nothing.
        }
        Ok(())
    }

    /// Synchronize all rollback items with remote counterparts.
    /// This method will be invoked by external workers to keep the local
    /// data in sync with the API. In theory it should not be necessary to
    /// do so, but in practice, and especially for alpha release, it is useful
    /// to have a way to recover from malfunctions.
    ///
    pub async fn sync_all<I>(
        api: &Session,
        tx: &mut impl RunTransaction,
        batch: I,
        queue: &Queue,
    ) -> Result<(), MailContextError>
    where
        I: Into<Option<usize>> + Copy,
    {
        Self::sync_labels(api, tx, batch, queue).await?;
        Self::sync_messages(api, tx, batch, queue).await?;
        Self::sync_conversations(api, tx, batch, queue).await?;

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn sync_labels<I>(
        api: &Session,
        tx: &mut impl RunTransaction,
        batch: I,
        queue: &Queue,
    ) -> Result<(), MailContextError>
    where
        I: Into<Option<usize>>,
    {
        Self::sync_items_impl::<LabelRollbackHandler, _>(api, tx, batch.into(), queue).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn sync_messages<I>(
        api: &Session,
        tx: &mut impl RunTransaction,
        batch: I,
        queue: &Queue,
    ) -> Result<(), MailContextError>
    where
        I: Into<Option<usize>>,
    {
        Self::sync_items_impl::<MessageRollbackHandler, _>(api, tx, batch.into(), queue).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn sync_conversations<I>(
        api: &Session,
        tx: &mut impl RunTransaction,
        batch: I,
        queue: &Queue,
    ) -> Result<(), MailContextError>
    where
        I: Into<Option<usize>>,
    {
        Self::sync_items_impl::<ConversationRollbackHandler, _>(api, tx, batch.into(), queue).await
    }

    /// This helper method is used to find all rollback items of a specific kind.
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
    async fn find_remote_ids_by_kind(
        kind: RollbackItemType,
        tether: &Tether,
    ) -> Result<Vec<String>, StashError> {
        tether
            .query_values::<_, String>(
                format!(
                    "SELECT remote_id FROM {} WHERE item_type = ?",
                    Self::table_name()
                ),
                params![kind],
            )
            .await
    }

    /// This helper method is used to delete rollback item of a specific kind & remote_id.
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
        api: &Session,
        tx_runner: &mut T,
        batch: Option<usize>,
        queue: &Queue,
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
            .map(async |ids| Ok((ids, H::fetch_items(api, ids).await?)));

        let mut tasks = FuturesUnordered::from_iter(batches);

        while let Some(result) = tasks.next().await {
            let (ids, mut items) = result.inspect_err(|e: &ApiServiceError| {
                error!("Failed to fetch batch ({:?}): {e:?}", H::item_type());
            })?;

            H::fetch_and_apply_dependencies(&mut items, api, tx_runner)
                .await
                .inspect_err(|e| {
                    error!(
                        "Failed to sync dependencies for batch ({:?}): {e:?}",
                        H::item_type()
                    )
                })?;

            let mut changeset = RebaseChangeSet::default();
            tx_runner
                .run_tx(async |tx| {
                    H::store_items(items, &mut changeset, tx)
                        .await
                        .inspect_err(|e| {
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

                    if let Err(e) = queue
                        .rebase_in(
                            proton_action_queue::action::ActionGroup::default(),
                            &changeset,
                            tx,
                        )
                        .await
                    {
                        tracing::error!("Failed to rebase: {e:?}");
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

trait RollbackHandler: 'static + Send + Sync {
    type Item: 'static;
    type RemoteId: ProtonIdMarker + From<String> + std::fmt::Display;

    fn item_type() -> RollbackItemType;

    fn fetch_items(
        api: &Session,
        remote_ids: &[Self::RemoteId],
    ) -> impl Future<Output = Result<Vec<Self::Item>, ApiServiceError>> + Send;

    fn fetch_and_apply_dependencies(
        items: &mut [Self::Item],
        api: &Session,
        tx_runner: &mut impl RunTransaction,
    ) -> impl Future<Output = Result<(), MailContextError>>;

    fn store_items(
        items: Vec<Self::Item>,
        changeset: &mut RebaseChangeSet,
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
        api: &Session,
        remote_ids: &[Self::RemoteId],
    ) -> Result<Vec<Self::Item>, ApiServiceError> {
        let options = GetMessagesOptions {
            page: 0,
            page_size: remote_ids.len() as u64,
            ids: Some(remote_ids.to_vec()),
            ..Default::default()
        };

        Ok(api.get_messages(options).await?.messages)
    }

    async fn fetch_and_apply_dependencies(
        items: &mut [Self::Item],
        api: &Session,
        tx_runner: &mut impl RunTransaction,
    ) -> Result<(), MailContextError> {
        let mut dependency_fetcher = DependencyFetcher::new();
        let tether = tx_runner.tether();
        for item in items.iter_mut() {
            dependency_fetcher
                .check_api_message_metadata(item, tether)
                .await?;
        }

        let unresolved_label_ids = dependency_fetcher.fetch_and_store(api, tx_runner).await?;

        for item in items.iter_mut() {
            item.label_ids
                .retain(|id| !unresolved_label_ids.contains(id));
        }

        Ok(())
    }

    async fn store_items(
        items: Vec<Self::Item>,
        changeset: &mut RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), MailContextError> {
        for item in items {
            if Message::sync_decision(&item, None, tx).await? == MessageSyncDecision::Skip {
                continue;
            }
            let mut m = Message::from_api_metadata(item, tx).await?;
            m.save(tx).await?;
            changeset.add(m.id());
        }

        Ok(())
    }
}

struct ConversationRollbackHandler {}

impl RollbackHandler for ConversationRollbackHandler {
    type Item = GetConversationResponse;
    type RemoteId = ConversationId;

    fn item_type() -> RollbackItemType {
        RollbackItemType::Conversation
    }

    async fn fetch_items(
        api: &Session,
        remote_ids: &[Self::RemoteId],
    ) -> Result<Vec<Self::Item>, ApiServiceError> {
        let iter = remote_ids.iter().map(|id| api.get_conversation(id.clone()));

        let mut tasks = FuturesOrdered::from_iter(iter);

        let mut result = Vec::with_capacity(remote_ids.len());
        while let Some(output) = tasks.next().await {
            match output {
                Ok(conversation) => result.push(conversation),
                Err(ApiServiceError::UnprocessableEntity(_, Some(ref info)))
                    if info.code == Mail::ConversationDoesNotExist as u32 =>
                {
                    // Conversation doesn't exist anymore - skip it.
                    // It will be cleaned up by the event loop when it processes the deletion event.
                    warn!(
                        "Conversation does not exist, skipping rollback: {:?}",
                        info.details
                    );
                }
                Err(e) => return Err(e),
            }
        }

        Ok(result)
    }

    async fn fetch_and_apply_dependencies(
        items: &mut [Self::Item],
        api: &Session,
        tx_runner: &mut impl RunTransaction,
    ) -> Result<(), MailContextError> {
        let mut dependency_fetcher = DependencyFetcher::new();
        let tether = tx_runner.tether();
        for item in items.iter_mut() {
            dependency_fetcher
                .check_api_conversation(&item.conversation, tether)
                .await?;
            for message in &item.messages {
                dependency_fetcher
                    .check_api_message_metadata(message, tether)
                    .await?;
            }
        }

        let unresolved_label_ids = dependency_fetcher.fetch_and_store(api, tx_runner).await?;

        for item in items.iter_mut() {
            item.conversation
                .labels
                .retain(|l| !unresolved_label_ids.contains(&l.id));
            for message in &mut item.messages {
                message
                    .label_ids
                    .retain(|id| !unresolved_label_ids.contains(id));
            }
        }

        Ok(())
    }

    async fn store_items(
        items: Vec<Self::Item>,
        changeset: &mut RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), MailContextError> {
        for item in items {
            let mut c = Conversation::from(item.conversation);
            c.save(tx).await?;
            changeset.add(c.id());

            let ids =
                Message::create_or_update_messages_from_metadata(item.messages, None, tx).await?;
            changeset.add_many(ids);
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
        api: &Session,
        remote_ids: &[Self::RemoteId],
    ) -> Result<Vec<Self::Item>, ApiServiceError> {
        Ok(api.get_labels_by_ids(remote_ids.to_vec()).await?.labels)
    }

    async fn fetch_and_apply_dependencies(
        items: &mut [Self::Item],
        api: &Session,
        tx_runner: &mut impl RunTransaction,
    ) -> Result<(), MailContextError> {
        use std::collections::HashSet;

        // Collect the IDs of labels being rolled back to avoid checking them as dependencies
        let rollback_label_ids: HashSet<LabelId> =
            items.iter().map(|label| label.id.clone()).collect();

        let mut dependency_fetcher = DependencyFetcher::new();
        let tether = tx_runner.tether();
        for item in items.iter_mut() {
            // Only check parent dependency if it's not already in the rollback set
            if let Some(parent_id) = &item.parent_id {
                if !rollback_label_ids.contains(parent_id) {
                    dependency_fetcher.check_label(item, tether).await?;
                }
            }
        }

        let unresolved_label_ids = dependency_fetcher.fetch_and_store(api, tx_runner).await?;

        for item in items.iter_mut() {
            if let Some(parent_id) = &item.parent_id {
                if unresolved_label_ids.contains(parent_id) {
                    warn!(
                        "Removing unresolved parent reference {} from label {}",
                        parent_id, item.id
                    );
                    item.parent_id = None;
                }
            }
        }

        Ok(())
    }

    async fn store_items(
        items: Vec<Self::Item>,
        changeset: &mut RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), MailContextError> {
        for item in items {
            let mut l = Label::from(item);
            l.save(tx).await?;
            changeset.add(l.id());
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
            }
        }
    }

    impl From<Conversation> for RollbackItem {
        fn from(conversation: Conversation) -> Self {
            Self::from(&conversation)
        }
    }
}
