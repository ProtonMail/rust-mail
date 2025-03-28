#[cfg(test)]
#[path = "../tests/models/rollback_item.rs"]
mod tests;

use crate::AppError;
use crate::datatypes::RollbackItemType;
use crate::models::{Conversation, Message, MessageBodyMetadata};
use futures::stream::{self, StreamExt, TryStreamExt};
use itertools::Itertools;
use proton_api_core::services::proton::LabelId;
use proton_api_core::services::proton::ProtonCore;
use proton_api_mail::services::proton::ProtonMail;
use proton_api_mail::services::proton::common::{ConversationId, MessageId};
use proton_api_mail::services::proton::requests::GetConversationsOptions;
use proton_api_mail::services::proton::responses::{GetConversationsResponse, GetMessageResponse};
use proton_core_common::models::Label;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use stash::{macros::Model, stash::Stash};
use tokio::sync::Mutex;
use tracing::{debug, error};

/// The number of concurrent requests to make when syncing rollback items.
///
/// Value was chosen arbitrarily. Could be put up to discussion.
const CONCURRENT_REQUEST_LIMIT: usize = 5;

/// Macro for generating synchronization code for any kind of rollback item.
/// Macro was chosen over a generic function because the type system would require
/// boundings that would make implementation of those boundings more time consuming than
/// its worth.
///
/// ## Parameters
///
/// * `$item` - The type of the item to sync. This is a token which allows the macro to put proper typing.
/// * `$class` - Implementation of the type which contains `save`.
/// * `$stash` - The local database instance to use for syncing.
/// * `$batch` - The number of items to sync in a single batch.
/// * `$api_request` - The API request to make to get the items. It is expected to be a clousure
///   that takes a `RemoteId` and returns a Future of API response.
/// * `$from_api_to_local` - The function to convert the API response to local items. It is expected to be a closure
///   that takes the API response and returns a Future of IntoIterator over models.
///
/// ## Errors
///
/// As sync_all method. This is only a helper macro to reduce code duplication.
///
macro_rules! sync_any {
    ($item:tt, $class:tt, $tether:expr, $stash: expr, $batch:expr => $api_request:expr => $from_api_to_local: expr) => {{
        let tether = $tether;
        let items = Self::find_by_kind(RollbackItemType::$item, tether).await?;
        let batch = $batch.into().unwrap_or(items.len() + 1);
        let chunked_remote_ids = items.into_iter().map(|item| item.remote_id).chunks(batch);

        stream::iter(&chunked_remote_ids)
            .then(|remote_ids| async {
                let items: Mutex<Vec<$class>> = Mutex::new(Vec::new());

                stream::iter(remote_ids)
                    .map(|remote_id| {
                        debug!(
                            "Syncing {} with remote ID {:?}",
                            stringify!($item),
                            remote_id
                        );
                        remote_id
                    })
                    .then($api_request)
                    .map_err(AppError::from)
                    .try_for_each_concurrent(CONCURRENT_REQUEST_LIMIT, |api_items| async {
                        let api_items = $from_api_to_local(api_items).await?;
                        items.lock().await.extend(api_items);

                        Ok(())
                    })
                    .await?;

                Ok(items.into_inner())
            })
            .try_for_each(move |mut items| {
                let mut tether = $stash.connection();
                {
                    async move {
                        tether
                            .tx(async |tx| {
                                for item in items.iter_mut() {
                                    let result = $class::save(item, &tx).await;

                                    if let Err(err) = result {
                                        error!(
                                            "Failed to save {} with remote ID {:?}: {:?}",
                                            stringify!($item),
                                            item.remote_id,
                                            err
                                        );

                                        return Err(err.into());
                                    }

                                    let result = Self::delete_by_rid_and_kind(
                                        item.remote_id.clone().map(|v| v.into_inner()),
                                        RollbackItemType::$item,
                                        &tx,
                                    )
                                    .await;

                                    if let Err(err) = result {
                                        error!(
                                            "Failed to delete {} with remote ID {:?}: {:?}",
                                            stringify!($item),
                                            item.remote_id,
                                            err
                                        );

                                        return Err(err.into());
                                    }

                                    debug!(
                                        "Synced {} with remote ID {:?}",
                                        stringify!($item),
                                        item.remote_id
                                    );
                                }

                                Result::<_, AppError>::Ok(())
                            })
                            .await
                    }
                }
            })
            .await?;

        Ok(())
    }};
}

/// A record of an action that was rolled back.
///
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("rollback_actions")]
pub struct RollbackItem {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and it is important for synchronization.
    #[IdField]
    pub remote_id: String,

    /// Table can store Labels, Messages, and Conversations.
    #[DbField]
    pub item_type: RollbackItemType,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
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
    /// * `api` - The API client to use for syncing.
    /// * `stash` - The local database instance to use for syncing.
    /// * `batch` - The number of items to sync in a single batch.
    ///
    /// ## Errors
    ///
    /// This method will return an error if any of the API requests fail.
    /// It will also return an error if any of the local database operations fail.
    /// Method cleans up the local database by deleting the records that have
    /// been synced, so double syncing should never happen.
    ///
    ///
    pub async fn sync_all<I, API>(api: &API, stash: &Stash, batch: I) -> Result<(), AppError>
    where
        I: Into<Option<usize>> + Copy,
        API: ProtonMail + ProtonCore,
    {
        let tether = stash.connection();
        Self::sync_labels(api, &tether, stash.clone(), batch).await?;
        Self::sync_messages(api, &tether, stash.clone(), batch).await?;
        Self::sync_conversations(api, &tether, stash.clone(), batch).await?;

        Ok(())
    }

    /// Synchronize all labels with remote counterparts.
    ///
    /// ## Parameters & Errors
    ///
    /// Look at the documentation of the `sync_all` method.
    ///
    pub async fn sync_labels<I, API>(
        api: &API,
        tether: &Tether,
        stash: Stash,
        batch: I,
    ) -> Result<(), AppError>
    where
        I: Into<Option<usize>>,
        API: ProtonCore,
    {
        use proton_api_core::services::proton::GetLabelsResponse;

        sync_any!(Label, Label, tether, stash, batch => |remote_id| async {
            api.get_labels_by_ids(vec![LabelId::from(remote_id)]).await
        } => |api_labels: GetLabelsResponse| async {
            Result::<_, AppError>::Ok(api_labels.labels.into_iter().map_into())
        })
    }

    /// Synchronize all messages with remote counterparts.
    ///
    /// ## Parameters & Errors
    ///
    /// Look at the documentation of the `sync_all` method.
    ///
    pub async fn sync_messages<I, PM>(
        api: &PM,
        tether: &Tether,
        stash: Stash,
        batch: I,
    ) -> Result<(), AppError>
    where
        I: Into<Option<usize>>,
        PM: ProtonMail,
    {
        sync_any!(Message, MessageAndBodyMetadata, tether, stash, batch => |remote_id| async {
            api.get_message(MessageId::from(remote_id)).await
        } => |api_message: GetMessageResponse| async {
            let remote_id = api_message.message.metadata.id.clone();
            let (metadata, body_metadata, _) = Message::from_api_data(api_message.message, tether).await?;
            Result::<_, AppError>::Ok(Some(MessageAndBodyMetadata{message_metadata: metadata,body_metadata,remote_id:Some(remote_id)}))
        })
    }

    /// Synchronize all conversations with remote counterparts.
    ///
    /// ## Parameters & Errors
    ///
    /// Look at the documentation of the `sync_all` method.
    ///
    pub async fn sync_conversations<I, PM>(
        api: &PM,
        tether: &Tether,
        stash: Stash,
        batch: I,
    ) -> Result<(), AppError>
    where
        I: Into<Option<usize>>,
        PM: ProtonMail,
    {
        sync_any!(Conversation, Conversation, tether, stash, batch => |remote_id| async {
            api.get_conversations(GetConversationsOptions {
                ids: Some(vec![ConversationId::from(remote_id)]),
                ..Default::default()
            }).await
        } => |api_conversations: GetConversationsResponse| async {
            Result::<_, AppError>::Ok(api_conversations.conversations.into_iter().map_into())
        })
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
    async fn find_by_kind(
        kind: RollbackItemType,
        tether: &Tether,
    ) -> Result<Vec<RollbackItem>, StashError> {
        RollbackItem::find("WHERE item_type = ?", params![kind], tether).await
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
}

// Wrapper type so both the body and metadata are synced.
struct MessageAndBodyMetadata {
    message_metadata: Message,
    body_metadata: MessageBodyMetadata,
    remote_id: Option<MessageId>,
}

impl MessageAndBodyMetadata {
    async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        self.message_metadata.save(bond).await?;
        self.body_metadata.save(bond).await?;
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
