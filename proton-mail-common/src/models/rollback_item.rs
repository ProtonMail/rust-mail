#[cfg(test)]
#[path = "../tests/models/rollback_item.rs"]
mod tests;

use crate::datatypes::ItemType;
use crate::models::{Conversation, Label, Message};
use crate::AppError;
use futures::stream::{self, StreamExt, TryStreamExt};
use itertools::Itertools;
use proton_api_mail::services::proton::requests::GetConversationsOptions;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::RemoteId;
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface, StashError, Tether};
use stash::{macros::Model, stash::Stash};
use tokio::sync::Mutex;

/// The number of concurrent requests to make when syncing rollback items.
///
/// Value was chosen arbitrarily. Could be put up to discussion.
const CONCURRENT_REQUEST_LIMIT: usize = 5;

/// A record of an action that was rolled back.
/// This record should be invoked only from action::rollback_local handler.
/// This is crucial to ensure RemoteId is present.
/// Otherwise application will panic.
///
/// ## Improvements
///
/// This model could expose a method to sync one item.
///
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("rollback_actions")]
pub struct RollbackItem {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and it is important for synchronization.
    #[IdField]
    pub remote_id: RemoteId,

    /// Table can store Labels, Messages, and Conversations.
    #[DbField]
    pub item_type: ItemType,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl RollbackItem {
    /// Synchronize all rollback items with remote counterparts.
    /// This method will be invoked by external workers to keep the local
    /// data in sync with the API. In theory it should not be necessary to
    /// do so, but in practice, and especially for alpha release, it is useful
    /// to have a way to recover from malfunctions.
    ///
    /// ## Errors
    ///
    /// This method will return an error if any of the API requests fail.
    /// It will also return an error if any of the local database operations fail.
    /// Method cleans up the local database by deleting the records that have
    /// been synced, so double syncing should never happen.
    ///
    /// ## Improvements
    ///
    /// This method could be improved by adding limit over how many items
    /// should be synced at once.
    ///
    pub async fn sync_all<PM: ProtonMail>(api: &PM, stash: &Stash) -> Result<(), AppError> {
        Self::sync_labels(api, stash).await?;
        Self::sync_messages(api, stash).await?;
        Self::sync_conversations(api, stash).await?;

        Ok(())
    }

    /// Synchronize all labels with remote counterparts.
    ///
    /// ## Errors
    ///
    /// Look at the documentation of the `sync_all` method.
    ///
    pub async fn sync_labels<PM: ProtonMail>(api: &PM, stash: &Stash) -> Result<(), AppError> {
        let labels = Self::find_by_kind(ItemType::Label, stash).await?;
        let remote_ids = labels.into_iter().map(|item| item.remote_id);
        let labels: Mutex<Vec<Label>> = Mutex::new(Vec::new());

        stream::iter(remote_ids)
            .then(|remote_id| api.get_labels_by_ids(vec![remote_id.into()]))
            .map_err(AppError::from)
            .try_for_each_concurrent(CONCURRENT_REQUEST_LIMIT, |api_labels| async {
                let api_labels = api_labels.labels.into_iter().map_into();
                labels.lock().await.extend(api_labels);

                Ok(())
            })
            .await?;

        for label in labels.lock().await.iter_mut() {
            let tx = stash.transaction().await?;
            Label::sync_label(label, &tx).await?;
            Self::delete_by_rid_and_kind(
                label.remote_id.clone().map(Into::into),
                ItemType::Label,
                &tx,
            )
            .await?;
            tx.commit().await?;
        }

        Ok(())
    }

    /// Synchronize all messages with remote counterparts.
    ///
    /// ## Errors
    ///
    /// Look at the documentation of the `sync_all` method.
    ///
    pub async fn sync_messages<PM: ProtonMail>(api: &PM, stash: &Stash) -> Result<(), AppError> {
        let messages = Self::find_by_kind(ItemType::Message, stash).await?;
        let remote_ids = messages.into_iter().map(|item| item.remote_id);
        let messages: Mutex<Vec<Message>> = Mutex::new(Vec::new());

        stream::iter(remote_ids)
            .then(|remote_id| api.get_message(remote_id.into()))
            .map_err(AppError::from)
            .try_for_each_concurrent(CONCURRENT_REQUEST_LIMIT, |api_message| async {
                let message = Message::from_api_data(api_message.message, stash).await?;

                messages.lock().await.push(message);

                Ok(())
            })
            .await?;

        for message in messages.lock().await.iter_mut() {
            let tx = stash.transaction().await?;
            Message::sync_message(message, &tx).await?;
            Self::delete_by_rid_and_kind(
                message.remote_id.clone().map(Into::into),
                ItemType::Message,
                &tx,
            )
            .await?;
            tx.commit().await?;
        }

        Ok(())
    }

    /// Synchronize all conversations with remote counterparts.
    ///
    /// ## Errors
    ///
    /// Look at the documentation of the `sync_all` method.
    ///
    pub async fn sync_conversations<PM: ProtonMail>(
        api: &PM,
        stash: &Stash,
    ) -> Result<(), AppError> {
        let conversations = Self::find_by_kind(ItemType::Conversation, stash).await?;
        let remote_ids = conversations.into_iter().map(|item| item.remote_id);
        let conversations: Mutex<Vec<Conversation>> = Mutex::new(Vec::new());

        stream::iter(remote_ids)
            .then(|remote_id| {
                api.get_conversations(GetConversationsOptions {
                    ids: Some(vec![remote_id.into()]),
                    ..Default::default()
                })
            })
            .map_err(AppError::from)
            .try_for_each_concurrent(CONCURRENT_REQUEST_LIMIT, |api_conversations| async {
                let api_conversations = api_conversations.conversations.into_iter().map_into();
                conversations.lock().await.extend(api_conversations);

                Ok(())
            })
            .await?;

        for conversation in conversations.lock().await.iter_mut() {
            let tx = stash.transaction().await?;
            Conversation::sync_conversation(conversation, &tx).await?;
            Self::delete_by_rid_and_kind(
                conversation.remote_id.clone(),
                ItemType::Conversation,
                &tx,
            )
            .await?;
            tx.commit().await?;
        }

        Ok(())
    }

    async fn find_by_kind<I: Into<AgnosticInterface> + Interface>(
        kind: ItemType,
        interface: &I,
    ) -> Result<Vec<RollbackItem>, StashError> {
        RollbackItem::find("WHERE item_type = ?", params![kind], interface, None).await
    }

    async fn delete_by_rid_and_kind(
        remote_id: Option<RemoteId>,
        kind: ItemType,
        stash: &Tether,
    ) -> Result<(), StashError> {
        stash
            .execute(
                format!(
                    "DELETE FROM {} WHERE remote_id = ? AND item_type = ?",
                    Self::table_name()
                ),
                params![remote_id, kind],
            )
            .await?;

        Ok(())
    }
}

impl<'a> From<&'a Label> for RollbackItem {
    fn from(label: &'a Label) -> Self {
        Self {
            // Label which action was rollback has to have remote_id.
            remote_id: label.remote_id.clone().map(RemoteId::from).unwrap(),
            item_type: ItemType::Label,
            row_id: None,
            stash: label.stash.clone(),
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
            // Message which action was rollback has to have remote_id.
            remote_id: message.remote_id.clone().unwrap(),
            item_type: ItemType::Message,
            row_id: None,
            stash: message.stash.clone(),
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
            // Conversation which action was rollback has to have remote_id.
            remote_id: conversation.remote_id.clone().unwrap(),
            item_type: ItemType::Conversation,
            row_id: None,
            stash: conversation.stash.clone(),
        }
    }
}

impl From<Conversation> for RollbackItem {
    fn from(conversation: Conversation) -> Self {
        Self::from(&conversation)
    }
}
