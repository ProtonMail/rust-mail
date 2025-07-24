pub mod addresses;
mod available_action;
pub mod conversations;
pub mod draft;
pub mod labels;
pub mod messages;
pub mod notifications_quick_actions;
pub mod refresh;
pub mod rollback;

pub use self::available_action::*;
use crate::actions::conversations::UndoLabelAsConversations;
use crate::actions::messages::UndoLabelAsMessages;
use crate::datatypes::RollbackItemType;
use crate::models::{Conversation, Message, RollbackItem};
use crate::{AppError, MailUserContext};
use addresses::{block, unblock, update_incoming_defaults};
use futures::future::{join, join_all};
use indoc::formatdoc;
use proton_action_queue::action::{Action, FactoryError, Handler, WriterGuard, WriterGuardError};
use proton_action_queue::queue::Queue;
use proton_core_api::consts::General;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{LabelId, Proton, ProtonIdMarker};
use proton_core_common::action_queue::CoreActionError;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, LabelError, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::response_data::OperationResult;
use proton_sqlite3::rusqlite::ToSql;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, StashError, Tether};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Weak;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum MailActionError {
    #[error("Http: {0}")]
    Http(#[from] ApiServiceError),
    #[error("Stash: {0}")]
    Stash(#[from] StashError),
    #[error("App: {0}")]
    App(#[from] AppError),
    #[error("Label: {0}")]
    Label(#[from] LabelError),
    #[error("No input provided")]
    NoInput,
    #[error("Queue Writer Guard Expired")]
    QueueWriterGuardExpired,
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

impl proton_action_queue::action::Error for MailActionError {
    fn is_network_failure(&self) -> bool {
        if let Self::Http(e) = self {
            e.is_network_failure()
        } else {
            false
        }
    }

    fn is_writer_guard_expired(&self) -> bool {
        matches!(self, Self::QueueWriterGuardExpired)
    }
}

impl From<WriterGuardError> for MailActionError {
    fn from(value: WriterGuardError) -> Self {
        match value {
            WriterGuardError::Expired => Self::QueueWriterGuardExpired,
            WriterGuardError::Stash(e) => Self::Stash(e),
        }
    }
}

impl From<anyhow::Error> for MailActionError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}

impl From<CoreActionError> for MailActionError {
    fn from(value: CoreActionError) -> Self {
        match value {
            CoreActionError::Http(api_service_error) => Self::Http(api_service_error),
            CoreActionError::Stash(stash_error) => Self::Stash(stash_error),
            CoreActionError::Label(label_error) => Self::Label(label_error),
            CoreActionError::NoInput => Self::NoInput,
            CoreActionError::QueueWriterGuardExpired => Self::QueueWriterGuardExpired,
            CoreActionError::Other(error) => Self::Other(error),
        }
    }
}

pub(crate) fn register_mail_actions(queue: &Queue, ctx: &Weak<MailUserContext>, api: &Proton) {
    fn register_action<T>(queue: &Queue, handler: T)
    where
        T: Handler,
        T::Action: Action<Handler = T>,
    {
        if let Err(e) = queue.register::<T::Action>(handler) {
            match e {
                FactoryError::AlreadyRegistered(_) => {
                    // Do nothing it is possible we already registered this action
                    // in the queue once before.
                }
                e => {
                    panic!("Failed to register action: {e:?}");
                }
            }
        }
    }

    register_action(queue, conversations::DeleteHandler { api: api.clone() });
    register_action(queue, conversations::UnlabelHandler { api: api.clone() });
    register_action(queue, conversations::LabelHandler { api: api.clone() });
    register_action(queue, conversations::MarkReadHandler { api: api.clone() });
    register_action(queue, conversations::MarkUnreadHandler { api: api.clone() });
    register_action(queue, conversations::PrefetchHandler { ctx: ctx.clone() });
    register_action(queue, block::BlockHandler { api: api.clone() });
    register_action(queue, unblock::UnblockHandler { api: api.clone() });
    register_action(
        queue,
        update_incoming_defaults::SyncIncomingDefaultsHandler { api: api.clone() },
    );
    register_action(queue, conversations::MoveHandler { api: api.clone() });
    register_action(
        queue,
        conversations::RefreshMetadataHandler { ctx: ctx.clone() },
    );
    register_action(queue, messages::LabelHandler { api: api.clone() });
    register_action(queue, messages::UnlabelHandler { api: api.clone() });
    register_action(queue, messages::MoveHandler { api: api.clone() });
    register_action(queue, messages::DeleteHandler { api: api.clone() });
    register_action(
        queue,
        messages::DeleteAllMessagesInLabelHandler { api: api.clone() },
    );
    register_action(queue, messages::ReadHandler { api: api.clone() });
    register_action(queue, messages::UnreadHandler { api: api.clone() });
    register_action(queue, messages::HamHandler { api: api.clone() });
    register_action(queue, messages::ReportPhishingHandler { ctx: ctx.clone() });
    register_action(queue, messages::PrefetchHandler { ctx: ctx.clone() });
    register_action(queue, messages::RefreshMetadataHandler { api: api.clone() });
    register_action(queue, draft::SaveHandler { ctx: ctx.clone() });
    register_action(queue, draft::SendHandler { ctx: ctx.clone() });
    register_action(queue, labels::ExpandHandler { api: api.clone() });
    register_action(queue, messages::LabelAsHandler { api: api.clone() });
    register_action(queue, conversations::LabelAsHandler { api: api.clone() });
    register_action(queue, draft::DiscardHandler { api: api.clone() });
    register_action(queue, draft::UndoSendHandler { api: api.clone() });
    register_action(queue, draft::AttachmentUploadHandler { ctx: ctx.clone() });
    register_action(queue, draft::AttachmentRemoveHandler { api: api.clone() });
    register_action(queue, refresh::ActionRefreshHandler { ctx: ctx.clone() });
    register_action(queue, rollback::RollbackActionHandler { ctx: ctx.clone() });
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound = "")]
struct GenericActionData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    target_ids: Vec<T::IdType>,
    remote_target_ids: Vec<T::RemoteId>,
    phantom: PhantomData<T>,
}

impl<T> GenericActionData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    pub fn new(target_ids: impl IntoIterator<Item = T::IdType>) -> Self {
        Self {
            target_ids: Vec::from_iter(target_ids),
            remote_target_ids: vec![],
            phantom: PhantomData,
        }
    }

    /// Resolve all remote ids.
    ///
    /// Resolved remote ids are stored on self.
    ///
    /// # Errors
    ///
    /// Returns error if ids could not be resolved.
    async fn resolve_ids(&mut self, tether: &Tether) -> Result<(), MailActionError> {
        if self.target_ids.is_empty() {
            return Err(MailActionError::NoInput);
        }

        let remote_target_ids = T::local_ids_counterpart(self.target_ids.clone(), tether)
            .await
            .map_err(|e| {
                error!("Failed to resolve ids: {e:?}");
                e
            })?;

        self.remote_target_ids = remote_target_ids;

        Ok(())
    }

    /// Return the ids of all the items which do not have a remote id.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    async fn unsynced_item_ids(&self, tether: &Tether) -> Result<Vec<T::IdType>, MailActionError> {
        let placeholders = stash::utils::placeholders_n(self.target_ids.len());
        #[allow(trivial_casts)]
        let values = self
            .target_ids
            .iter()
            .map(|id| Box::new(id.clone()) as Box<dyn ToSql + Send>)
            .collect();
        Ok(tether
            .query_values::<_, T::IdType>(
                formatdoc!(
                    "
                            SELECT
                                {} AS value
                            FROM
                                {}
                            WHERE
                                {} IN ({})
                            AND
                                {} IS NULL
                            ",
                    T::id_field_name(),
                    T::table_name(),
                    T::id_field_name(),
                    placeholders,
                    T::remote_id_field_name(),
                ),
                values,
            )
            .await?)
    }

    /// Mark the action items to be rollback
    async fn mark_rollback(
        &self,
        item_type: RollbackItemType,
        tx: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        for remote_id in self.remote_target_ids.iter() {
            RollbackItem::new(remote_id.to_string(), item_type)
                .save(tx)
                .await?;
        }

        Ok(())
    }
}

/// Convenience type which contains data common to many actions.
/// It
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound = "")]
struct GenericLabelRelatedActionData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    /// Local label id which this action applies to.
    label_id: LocalLabelId,
    /// Resolved remote label id.
    ///
    /// Note: this is only for user with remote execution, it should be set by then.
    remote_label_id: Option<LabelId>,
    /// Generic data
    data: GenericActionData<T>,
}

impl<T> GenericLabelRelatedActionData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    /// Create a new instance with the given `label_id` and target `ids`.
    pub fn new(label_id: LocalLabelId, target_ids: impl IntoIterator<Item = T::IdType>) -> Self {
        Self {
            label_id,
            remote_label_id: None,
            data: GenericActionData::new(target_ids),
        }
    }

    /// Resolve all remote ids.
    ///
    /// Resolved remote ids are stored on self.
    ///
    /// # Errors
    ///
    /// Returns error if ids could not be resolved.
    async fn resolve_ids(&mut self, tether: &Tether) -> Result<(), MailActionError> {
        self.data.resolve_ids(tether).await?;

        self.remote_label_id = Some(Label::resolve_remote_label_id(self.label_id, tether).await?);

        Ok(())
    }

    /// Return the ids of all the items which do not have a remote id.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    async fn unsynced_item_ids(&self, tether: &Tether) -> Result<Vec<T::IdType>, MailActionError> {
        self.data.unsynced_item_ids(tether).await
    }

    /// Mark the action items to be rollback
    async fn mark_rollback(
        &self,
        item_type: RollbackItemType,
        tx: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        self.data.mark_rollback(item_type, tx).await?;

        Ok(())
    }
}

/// Filter server response on which the operation failed.
pub fn filter_responses<T: ProtonIdMarker>(responses: Vec<OperationResult<T>>) -> Vec<T> {
    filter_responses_by_codes(responses, &[General::NoError as u32])
}

/// Filter server response on which the operation failed.
pub fn filter_responses_by_codes<T: ProtonIdMarker>(
    responses: Vec<OperationResult<T>>,
    accepted: &[u32],
) -> Vec<T> {
    responses
        .into_iter()
        .filter(|r| !accepted.contains(&r.response.code))
        .map(|r| r.id)
        .collect::<Vec<_>>()
}

/// Action which moves target items between two labels.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActionMoveData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    /// The current label whether the items are locate.
    source_label_id: LocalLabelId,
    /// The destination label where the items should move to.
    destination_label_id: LocalLabelId,
    /// Resolved remote id for the destination label.
    remote_destination_label_id: Option<LabelId>,
    /// Local item ids that need to be moved.
    target_ids: Vec<T::IdType>,
    /// Resolved remote conversation ids.
    remote_target_ids: Vec<T::RemoteId>,
    phantom_data: PhantomData<T>,
}

impl<T> ActionMoveData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    /// Create a new action which moves items with `target_ids` from `source_label_id` to
    ///`destination_label_id`.
    pub fn new(
        source_label_id: LocalLabelId,
        destination_label_id: LocalLabelId,
        target_ids: impl IntoIterator<Item = T::IdType>,
    ) -> Self {
        Self {
            source_label_id,
            destination_label_id,
            remote_destination_label_id: None,
            target_ids: Vec::from_iter(target_ids),
            remote_target_ids: vec![],
            phantom_data: PhantomData,
        }
    }

    /// Resolve all remote ids
    ///
    /// # Errors
    ///
    /// * if some id can not be resolved
    async fn resolve_ids(&mut self, tx: &Bond<'_>) -> Result<(), MailActionError> {
        self.remote_destination_label_id =
            Some(Label::resolve_remote_label_id(self.destination_label_id, tx).await?);
        self.remote_target_ids = T::local_ids_counterpart(self.target_ids.clone(), tx)
            .await
            .inspect_err(|e| error!("Failed to resolve ids: {e:?}"))?;
        Ok(())
    }
}

#[allow(async_fn_in_trait, reason = "not used across threads")]
pub trait LabelAsTrait:
    ModelIdExtension<IdType: Copy + Hash + Eq + DeserializeOwned + Serialize, RemoteId: Display>
{
    const ROLLBACK_ITEM_TYPE: RollbackItemType;

    async fn apply_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError>;

    async fn remove_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError>;

    async fn remote_label(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Vec<Self::RemoteId>;

    async fn remote_unlabel(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Vec<Self::RemoteId>;
}

impl LabelAsTrait for Message {
    const ROLLBACK_ITEM_TYPE: RollbackItemType = RollbackItemType::Message;

    async fn apply_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        Self::apply_label(local_label_id, ids, bond).await
    }

    async fn remove_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        Self::remove_label(local_label_id, ids, bond).await
    }

    async fn remote_label(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Vec<Self::RemoteId> {
        match api.put_messages_label(ids.clone(), label_id, None).await {
            Ok(res) => filter_responses(res.responses),
            Err(e) => {
                error!("Failed to add message to label {e:?}");
                ids
            }
        }
    }

    async fn remote_unlabel(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Vec<Self::RemoteId> {
        match api.put_messages_unlabel(ids.clone(), label_id).await {
            Ok(res) => filter_responses(res.responses),
            Err(e) => {
                error!("Failed to remove message from label {e:?}");
                ids
            }
        }
    }
}

impl LabelAsTrait for Conversation {
    const ROLLBACK_ITEM_TYPE: RollbackItemType = RollbackItemType::Conversation;

    async fn apply_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        Self::apply_label(local_label_id, ids, bond).await
    }

    async fn remove_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        Self::remove_label(local_label_id, ids, bond).await
    }

    async fn remote_label(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Vec<Self::RemoteId> {
        match api
            .put_conversations_label(ids.clone(), label_id, None)
            .await
        {
            Ok(res) => filter_responses(res.responses),
            Err(e) => {
                error!("Failed to add message to label {e:?}");
                ids
            }
        }
    }

    async fn remote_unlabel(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Vec<Self::RemoteId> {
        match api.put_conversations_unlabel(ids.clone(), label_id).await {
            Ok(res) => filter_responses(res.responses),
            Err(e) => {
                error!("Failed to remove message from label {e:?}");
                ids
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelPair<T> {
    pub label: LocalLabelId,
    pub id: T,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAsData<T: LabelAsTrait> {
    source_label_id: LocalLabelId,
    add: Vec<LabelPair<T::IdType>>,
    remove: Vec<LabelPair<T::IdType>>,
}

impl<T: LabelAsTrait> LabelAsData<T> {
    pub fn new(
        cartesian: HashSet<LabelPair<T::IdType>>,
        source_label_id: LocalLabelId,
        items: Vec<T::IdType>,
        selected_label_ids: &[LocalLabelId],
        partially_selected_label_ids: &[LocalLabelId],
        all_label_ids: &[LocalLabelId],
    ) -> Self {
        // The way this works is simple.
        // 1. We figure out all existing (label, item) pairs
        // 2. Fully selected labels must end up in a state where everything is selected, we need to
        //    figure out the set difference so that we can revert it later (and minimize queries
        //    and API calls).
        // 3. If a label is neither selected or partially selected it should be removed. Same
        //    rationale as above.
        let mut add = vec![];
        let mut remove = vec![];

        for &label in all_label_ids {
            if selected_label_ids.contains(&label) {
                // Label these items if they haven't been labeled yet.
                for &id in &items {
                    let pair = LabelPair { label, id };
                    if !cartesian.contains(&pair) {
                        add.push(pair);
                    }
                }
            } else if partially_selected_label_ids.contains(&label) {
                // do nothing, keep label as is
            } else {
                // No selection: Remove
                for &id in &items {
                    let pair = LabelPair { label, id };
                    if cartesian.contains(&pair) {
                        remove.push(pair);
                    }
                }
            }
        }

        Self {
            add,
            remove,
            source_label_id,
        }
    }

    async fn apply_local_common(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        let (add, remove) = self.segregate_label();

        for (label, ids) in add {
            T::apply_label(label, ids, tx).await?;
        }

        for (label, ids) in remove {
            T::remove_label(label, ids, tx).await?;
        }
        Ok(())
    }

    async fn revert_local(&mut self, tx: &Bond<'_>) -> Result<(), StashError> {
        let (add, remove) = self.segregate_label();

        for (label, ids) in add {
            T::remove_label(label, ids.iter().copied(), tx).await?;
            let ids = T::local_ids_counterpart(ids, tx).await?;
            RollbackItem::save_many(tx, ids, T::ROLLBACK_ITEM_TYPE).await?;
        }

        for (label, ids) in remove {
            T::apply_label(label, ids.iter().copied(), tx).await?;
            let ids = T::local_ids_counterpart(ids, tx).await?;
            RollbackItem::save_many(tx, ids, T::ROLLBACK_ITEM_TYPE).await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        api: &Proton,
        mut guard: WriterGuard<'_>,
    ) -> Result<(), MailActionError> {
        let tether = guard.tether();
        let (add, remove) = self.segregate_label();
        let mut add_requests = vec![];

        for (label, messages) in add {
            let label = Label::resolve_remote_label_id(label, tether).await?;
            let messages = T::local_ids_counterpart(messages, tether).await?;

            for chunk in messages.chunks(150) {
                let chunk = chunk.to_owned();
                let label = label.clone();

                add_requests.push(T::remote_label(api, chunk, label));
            }
        }

        let mut remove_requests = vec![];

        for (label, messages) in remove {
            let label = Label::resolve_remote_label_id(label, tether).await?;
            let messages = T::local_ids_counterpart(messages, tether).await?;

            for chunk in messages.chunks(150) {
                let chunk = chunk.to_owned();
                let label = label.clone();

                remove_requests.push(T::remote_unlabel(api, chunk, label));
            }
        }

        let (add_fails, remove_fails) =
            join(join_all(add_requests), join_all(remove_requests)).await;

        let items = add_fails.into_iter().chain(remove_fails).flatten();

        guard
            .tx::<_, _, anyhow::Error>(async move |tx| {
                RollbackItem::save_many(tx, items, T::ROLLBACK_ITEM_TYPE).await?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    #[allow(
        clippy::type_complexity,
        reason = "It's clear due to how it's used in context"
    )]
    fn segregate_label(
        &self,
    ) -> (
        HashMap<LocalLabelId, Vec<T::IdType>>,
        HashMap<LocalLabelId, Vec<T::IdType>>,
    ) {
        let mut add = HashMap::<_, Vec<_>>::new();
        for &LabelPair { label, id } in &self.add {
            add.entry(label).or_default().push(id);
        }

        let mut remove = HashMap::<_, Vec<_>>::new();
        for &LabelPair { label, id } in &self.remove {
            remove.entry(label).or_default().push(id);
        }

        (add, remove)
    }
}

pub enum UndoLabelAs {
    Messages(UndoLabelAsMessages),
    Conversations(UndoLabelAsConversations),
}

impl UndoLabelAs {
    pub async fn undo(self, queue: &Queue, tether: &Tether) -> Result<(), AppError> {
        tracing::info!("undoing!");
        match self {
            UndoLabelAs::Messages(u) => u.undo(queue, tether).await,
            UndoLabelAs::Conversations(u) => u.undo(queue, tether).await,
        }
    }
}

pub struct LabelAsOutput {
    pub input_label_is_empty: bool,
    pub undo: UndoLabelAs,
}
