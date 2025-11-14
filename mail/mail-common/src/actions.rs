pub mod addresses;
mod available_action;
pub mod conversations;
pub mod draft;
pub mod generic_mobile_actions;
pub mod labels;
pub mod mail_settings;
pub mod messages;
pub mod mobile_actions_builder;
pub mod notifications_quick_actions;
pub mod refresh;
pub mod rollback;

pub use self::available_action::*;
pub use self::generic_mobile_actions::*;
pub use self::mobile_actions_builder::*;
use crate::actions::conversations::label_as::UndoLabelAsConversations;
use crate::actions::conversations::r#move::UndoMoveToConversations;
use crate::actions::messages::UndoLabelAsMessages;
use crate::actions::messages::UndoMoveToMessages;
use crate::actions::notifications_quick_actions::PushNotificationActionHandler;
use crate::datatypes::{LocalConversationId, LocalMessageId};
use crate::datatypes::{RollbackItemType, SystemLabelId};
use crate::models::RollbackItem;
use crate::models::{MailLabel, Message};
use crate::{AppError, MailUserContext};
use addresses::{block, unblock, update_incoming_defaults};
use anyhow::{Context, anyhow};
use indoc::formatdoc;
use proton_action_queue::action::{
    self, ActionDependencyKey, ActionDependencyKeys, ActionGroup, ActionId, FactoryError, Handler,
    WriterGuard, WriterGuardError,
};
use proton_action_queue::queue::{ActionRequeueReason, Queue};
use proton_action_queue::rebase::{RebaseChangeSet, RebaseKey};
use proton_core_api::consts::General;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{LabelId, ProtonIdMarker};
use proton_core_api::session::Session;
use proton_core_common::Origin;
use proton_core_common::action_queue::CoreActionError;
use proton_core_common::actions::dependency_builder::{
    ActionDependencyKeysBuilder, LocalIdActionDepExt,
};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{Label, LabelError, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::response_data::OperationResult;
use proton_sqlite3::rusqlite::ToSql;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use stash::exports::{Connection, Transaction};
use stash::orm::Model;
use stash::rusqlite::params_from_iter;
use stash::stash::{Bond, StashError, Tether};
use std::any::type_name;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Weak;
use tracing::error;

pub const PREFETCH_ROLLBACK_ACTION_GROUP: ActionGroup = ActionGroup::new("MAIL_PREFETCH_ROLLBACK");

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
    #[error("Lost context")]
    LostContext,
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

impl action::Error for MailActionError {
    fn can_requeue(&self) -> Option<ActionRequeueReason> {
        match self {
            Self::Http(e) if e.is_network_failure() => Some(ActionRequeueReason::NetworkFailed),
            Self::QueueWriterGuardExpired => Some(ActionRequeueReason::GuardExpired),
            Self::LostContext => Some(ActionRequeueReason::LostContext),
            _ => None,
        }
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

pub(crate) fn register_actions(
    queue: &Queue,
    origin: Origin,
    ctx: &Weak<MailUserContext>,
    api: &Session,
    http_client: &reqwest::Client,
) {
    fn reg<T>(queue: &Queue, handler: T)
    where
        T: Handler,
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

    fn replace<T>(queue: &Queue, handler: T)
    where
        T: Handler,
    {
        queue.register_or_replace::<T::Action>(handler);
    }

    match origin {
        Origin::App => {
            reg(queue, conversations::DeleteHandler { api: api.clone() });
            reg(queue, conversations::MarkReadHandler { api: api.clone() });
            reg(queue, conversations::MarkUnreadHandler { api: api.clone() });
            replace(queue, conversations::PrefetchHandler { ctx: ctx.clone() });
            reg(queue, conversations::SnoozeHandler { api: api.clone() });
            reg(queue, conversations::UnsnoozeHandler { api: api.clone() });
            reg(queue, block::BlockHandler { api: api.clone() });
            reg(queue, unblock::UnblockHandler { api: api.clone() });
            reg(
                queue,
                update_incoming_defaults::SyncIncomingDefaultsHandler {
                    api: api.clone(),
                    ctx: ctx.clone(),
                },
            );
            reg(queue, conversations::MoveHandler { api: api.clone() });
            replace(
                queue,
                conversations::RefreshMetadataHandler { ctx: ctx.clone() },
            );
            reg(queue, messages::MoveHandler { api: api.clone() });
            reg(queue, messages::DeleteHandler { api: api.clone() });
            reg(
                queue,
                messages::DeleteAllMessagesInLabelHandler { api: api.clone() },
            );
            reg(queue, messages::ReadHandler { api: api.clone() });
            reg(queue, messages::UnreadHandler { api: api.clone() });
            reg(queue, messages::HamHandler { api: api.clone() });
            replace(queue, messages::ReportPhishingHandler { ctx: ctx.clone() });
            replace(queue, messages::PrefetchHandler { ctx: ctx.clone() });
            reg(queue, messages::RefreshMetadataHandler { api: api.clone() });
            reg(
                queue,
                messages::UnsubscribeNewsletterHandler {
                    http_client: http_client.clone(),
                    api: api.clone(),
                },
            );
            replace(queue, draft::SaveHandler { ctx: ctx.clone() });
            replace(queue, draft::SendHandler { ctx: ctx.clone() });
            reg(queue, labels::ExpandHandler { api: api.clone() });
            reg(queue, messages::LabelAsHandler { api: api.clone() });
            reg(queue, conversations::LabelAsHandler { api: api.clone() });
            reg(queue, draft::DiscardHandler { api: api.clone() });
            reg(queue, draft::UndoSendHandler { api: api.clone() });
            replace(queue, draft::AttachmentUploadHandler { ctx: ctx.clone() });
            reg(queue, draft::AttachmentRemoveHandler { api: api.clone() });
            replace(queue, refresh::ActionRefreshHandler { ctx: ctx.clone() });
            reg(queue, rollback::RollbackActionHandler { api: api.clone() });
            reg(
                queue,
                mail_settings::UpdateMobileActionsHandler { api: api.clone() },
            );
            reg(
                queue,
                mail_settings::UpdateNextMessageOnMoveHandler { api: api.clone() },
            );
            reg(queue, PushNotificationActionHandler { api: api.clone() });
            reg(
                queue,
                draft::AttachmentDispositionUpdateHandler { api: api.clone() },
            );
        }

        Origin::ShareExt => {
            replace(queue, draft::SaveHandler { ctx: ctx.clone() });
            replace(queue, draft::SendHandler { ctx: ctx.clone() });
            reg(queue, draft::DiscardHandler { api: api.clone() });
            replace(queue, draft::AttachmentUploadHandler { ctx: ctx.clone() });
            reg(queue, draft::AttachmentRemoveHandler { api: api.clone() });
            reg(
                queue,
                draft::AttachmentDispositionUpdateHandler { api: api.clone() },
            );
        }
    }
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
                                {}
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

impl<T> GenericActionData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned + LocalIdActionDepExt>,
{
    fn read_unread_action_dependency_keys(&self) -> ActionDependencyKeysBuilder {
        ActionDependencyKeysBuilder::new()
            .with_optional_many_ext(self.target_ids.iter().copied())
            .with_required_many(mark_read_unread_action_dependency_key(
                self.target_ids.iter().copied(),
            ))
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

impl<T> GenericLabelRelatedActionData<T>
where
    T: ModelIdExtension<IdType: LocalIdActionDepExt + Serialize + DeserializeOwned>,
{
    fn action_dependency_keys_builder_optional(&self) -> ActionDependencyKeysBuilder {
        ActionDependencyKeysBuilder::new()
            .with_optional_many_ext(self.data.target_ids.iter().copied())
            .with_required_related(self.label_id)
    }

    fn snooze_unsnooze_action_dependency_keys(&self) -> ActionDependencyKeysBuilder {
        self.action_dependency_keys_builder_optional()
            .with_required_many(snooze_unsnooze_action_dependency_key(
                self.data.target_ids.iter().copied(),
            ))
    }

    fn read_unread_action_dependency_keys(&self) -> ActionDependencyKeysBuilder {
        self.action_dependency_keys_builder_optional()
            // undo if a mark-read-unread dependency chain fails
            .with_required_many(mark_read_unread_action_dependency_key(
                self.data.target_ids.iter().copied(),
            ))
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

struct ActionMoveDataV2Ctx {
    trash_id: LocalLabelId,
    almost_all_mail_id: LocalLabelId,
    spam_id: LocalLabelId,
}

impl ActionMoveDataV2Ctx {
    fn new(tether: &Connection) -> Result<Self, StashError> {
        let spam_id = LabelId::spam().local_id(tether)?;
        let trash_id = LabelId::trash().local_id(tether)?;
        let almost_all_mail_id = LabelId::almost_all_mail().local_id(tether)?;

        Ok(Self {
            trash_id,
            spam_id,
            almost_all_mail_id,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
struct ActionMoveDataV2Entry {
    original_locations: Vec<LocalLabelId>,
    removed_labels: Vec<LabelPair<LocalMessageId>>,
    applied_labels: Vec<LabelPair<LocalMessageId>>,
    marked_read: Vec<LocalMessageId>,
    is_noop: bool,
}
impl ActionMoveDataV2Entry {
    fn revert(&self, tx: &Transaction<'_>) -> Result<(), StashError> {
        if !self.marked_read.is_empty() {
            Message::mark_read_or_unread(false, &self.marked_read, tx)?;
        }

        for label_pair in &self.removed_labels {
            Message::apply_label(label_pair.label, [label_pair.id], tx)?;
        }

        for label_pair in &self.applied_labels {
            Message::remove_label(label_pair.label, [label_pair.id], tx)?;
        }

        Ok(())
    }

    fn rebase<T: ConversationOrMessage>(
        &mut self,
        ctx: &ActionMoveDataV2Ctx,
        destination: Option<LocalLabelId>,
        id: T::IdType,
        tx: &Transaction<'_>,
    ) -> Result<(), StashError> {
        tracing::info!("Rebasing {id:?}");
        self.removed_labels.clear();
        self.applied_labels.clear();
        self.marked_read.clear();
        self.is_noop = false;
        let target = T::load_by_id_sync(id, tx)
            .with_context(|| format!("Could not find {}", type_name::<T>()))?
            .ok_or_else(|| StashError::Custom(anyhow!("Could not find {id:?}")))?;
        self.original_locations = target.get_exclusive_locations();

        self.move_to::<T>(ctx, destination, id, tx)
    }

    fn move_to<T: ConversationOrMessage>(
        &mut self,
        ctx: &ActionMoveDataV2Ctx,
        destination: Option<LocalLabelId>,
        id: T::IdType,
        tx: &Transaction<'_>,
    ) -> Result<(), StashError> {
        if destination == Some(ctx.trash_id) {
            self.marked_read = T::mark_read([id], tx)?;
        }

        if let Some(destination) = destination
            && [ctx.trash_id, ctx.spam_id].contains(&destination)
        {
            self.removed_labels = T::remove_all_removable_labels(&[id], tx)?;
        } else {
            // If there are  no source labels, it means that this msg/conv is
            // being moved from AllMail into somewhere else (e.g. because
            // its parent folder got deleted and this object has no
            // exclusive location anymore).
            //
            // In cases like these we don't want to remove the AllMail label
            // since the object is not actually /moved/ out of AllMail.
            for source_id in &self.original_locations {
                if let Some(source_label) = Label::load_by_id_sync(*source_id, tx)? {
                    let is_snoozed =
                        SystemLabel::new(&source_label).is_some_and(|label| label.is_snoozed());
                    if source_label.is_movable_out_of_folder() || is_snoozed {
                        let removed = T::remove_label(*source_id, [id], tx)
                            .context("Failed to remove source label")?;
                        self.removed_labels
                            .extend(removed.into_iter().map(|id| LabelPair {
                                label: source_label.id(),
                                id,
                            }));
                    }
                }

                if [ctx.trash_id, ctx.spam_id].contains(source_id) {
                    let applied = T::apply_label(ctx.almost_all_mail_id, [id], tx)
                        .context("Failed to add conversations to almost_all_mail")?;
                    self.applied_labels
                        .extend(applied.into_iter().map(|id| LabelPair {
                            label: ctx.almost_all_mail_id,
                            id,
                        }));
                }
            }
        }

        if let Some(destination) = destination {
            let applied = T::apply_label(destination, [id], tx)
                .context("Failed to apply destination label")?;
            self.applied_labels
                .extend(applied.into_iter().map(|id| LabelPair {
                    label: destination,
                    id,
                }));
        } else {
            // If there's no destination label, it means that this object is
            // being moved into AllMail.
            //
            // This doesn't make sense as an action on its own[1], but it
            // can happen when user undoes a move _from_ AllMail to Inbox,
            // for example; this is simply a no-op then.
            //
            // [1] after all, by definition all mails are in AllMail anyway
        }

        self.is_noop = if self.marked_read.is_empty() {
            let removed_label_ids = self.removed_labels.iter().collect::<HashSet<_>>();
            let applied_label_ids = self.applied_labels.iter().collect::<HashSet<_>>();
            removed_label_ids
                .difference(&applied_label_ids)
                .next()
                .is_none()
        } else {
            false
        };

        Ok(())
    }

    pub fn is_skippable(&self) -> bool {
        self.is_noop
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
/*
#[serde(bound(
    deserialize = "<T as Model>::IdType : DeserializeOwned",
    serialize = "<T as Model>::IdType : Serialize"
))]*/
pub struct ActionMoveData<T>
where
    T: ConversationOrMessage,
{
    destination: Option<LocalLabelId>,
    entries: HashMap<T::IdType, ActionMoveDataV2Entry>,
}

// Trait to handle conversion of the unread message state stored in the previous action
pub trait ActionMoveV1Compatability {
    fn into_local_message_id(self) -> Option<LocalMessageId>;
}

impl ActionMoveV1Compatability for LocalMessageId {
    fn into_local_message_id(self) -> Option<LocalMessageId> {
        Some(self)
    }
}

impl ActionMoveV1Compatability for LocalConversationId {
    fn into_local_message_id(self) -> Option<LocalMessageId> {
        // Unfortunately we don't have any good way to detect this during migration
        // since we don't have db access. What this means is that if we need to revert/undo
        // for this queue action some things maybe out of date.
        // TODO: Do we really care?
        None
    }
}

impl<T> ActionMoveData<T>
where
    T: ConversationOrMessage,
    <T as Model>::IdType: ActionMoveV1Compatability,
{
    fn from_action_move_data(action: ActionMoveDataV1<T>) -> Self {
        let mut entries = HashMap::with_capacity(action.sources.len());
        for (label_id, ids) in action.sources {
            for id in ids {
                let (removed_labels, unread) =
                    if let Some(local_msg_id) = id.into_local_message_id() {
                        let removed_labels = action
                            .removed_labels
                            .iter()
                            .filter_map(|v| {
                                (v.id != id).then_some(LabelPair {
                                    label: v.label,
                                    id: local_msg_id,
                                })
                            })
                            .collect::<Vec<_>>();
                        (
                            removed_labels,
                            if action.marked_read.contains(&local_msg_id) {
                                vec![local_msg_id]
                            } else {
                                vec![]
                            },
                        )
                    } else {
                        (vec![], vec![])
                    };

                entries.insert(
                    id,
                    ActionMoveDataV2Entry {
                        original_locations: label_id.map_or(vec![], |l| vec![l]),
                        removed_labels,
                        marked_read: unread,
                        applied_labels: vec![],
                        is_noop: false,
                    },
                );
            }
        }

        Self {
            destination: action.destination,
            entries,
        }
    }
    pub(crate) fn convert(old_version: u32, data: &[u8]) -> action::FactoryResult<Self> {
        #[allow(dead_code)]
        #[derive(Deserialize)]
        struct OldAction<T: ConversationOrMessage> {
            source_label_id: LocalLabelId,
            destination_label_id: LocalLabelId,
            target_ids: Vec<T::IdType>,
        }

        match old_version {
            1 => {
                let data = action::deserialize::<OldAction<T>>(data)?;

                let mut sources = HashMap::new();
                sources.insert(Some(data.source_label_id), data.target_ids);

                Ok(ActionMoveData::from_action_move_data(
                    ActionMoveDataV1::<T> {
                        destination: Some(data.destination_label_id),
                        sources,
                        marked_read: vec![],
                        removed_labels: vec![],
                    },
                ))
            }

            2 => Ok(ActionMoveData::from_action_move_data(
                action::deserialize::<ActionMoveDataV1<T>>(data)?,
            )),

            3 => Ok(action::deserialize::<Self>(data)?),

            other_version => Err(FactoryError::InvalidVersion(other_version)),
        }
    }
}

impl<T> ActionMoveData<T>
where
    T: ConversationOrMessage,
    <T as Model>::IdType: Into<RebaseKey>,
{
    /// Creates an action that moves `target_ids` from their exclusive locations
    /// into `destination`.
    pub async fn new(
        tether: &Tether,
        destination: LocalLabelId,
        target_ids: impl IntoIterator<Item = T::IdType>,
    ) -> Result<Option<Self>, StashError> {
        let mut entries = HashMap::new();

        for target_id in target_ids {
            let target = T::load(target_id, tether)
                .await?
                .with_context(|| format!("Could not find {}", type_name::<T>()))?;

            entries.insert(
                target_id,
                ActionMoveDataV2Entry {
                    original_locations: target.get_exclusive_locations(),
                    removed_labels: vec![],
                    marked_read: vec![],
                    applied_labels: vec![],
                    is_noop: false,
                },
            );
        }

        if entries.is_empty() {
            return Ok(None); // Don't queue an action unnecessarily
        }

        Ok(Some(Self {
            destination: Some(destination),
            entries,
        }))
    }

    async fn move_to_async(&mut self, bond: &Bond<'_>) -> anyhow::Result<()> {
        //TODO: handle revert
        // This action modifies self, so we need to send it and get it back.
        let mut this = self.clone();
        let this = bond
            .sync_bridge(move |tx| {
                this.move_to(tx)?;
                Ok(this)
            })
            .await?;

        *self = this;
        Ok(())
    }

    fn move_to(&mut self, tx: &Transaction<'_>) -> anyhow::Result<()> {
        let ctx = ActionMoveDataV2Ctx::new(tx)?;

        for (id, data) in &mut self.entries {
            data.move_to::<T>(&ctx, self.destination, *id, tx)?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        api: &Session,
        mut guard: WriterGuard<'_>,
    ) -> Result<(), MailActionError> {
        //TODO: handle revert
        let Some(dest_label) = self.destination else {
            return Ok(());
        };

        let tether = guard.tether();

        let dest_label = Label::resolve_remote_label_id(dest_label, tether).await?;
        let all_remote_ids = T::local_ids_counterpart(
            self.entries
                .iter()
                .filter_map(|(k, v)| {
                    // remove any item that has 0 changes, which means there is nothing to do
                    // and we should not communicate this to the server.
                    if v.is_skippable() {
                        tracing::info!("Skipping {k:?} due to noop");
                        None
                    } else {
                        Some(*k)
                    }
                })
                .collect::<Vec<_>>(),
            tether,
        )
        .await?;

        let failed = T::api_apply_label(api, all_remote_ids, dest_label.clone()).await?;
        if !failed.is_empty() {
            guard
                .tx::<_, _, anyhow::Error>(async move |tx| {
                    RollbackItem::save_many(tx, failed, T::ROLLBACK_ITEM_TYPE).await?;
                    Ok(())
                })
                .await?;
        }
        Ok(())
    }

    async fn revert_local(&mut self, tx: &Bond<'_>) -> Result<(), MailActionError> {
        //TODO: handle revert
        let this = self.clone();
        tx.sync_bridge(move |tx| {
            for (_, data) in this.entries {
                data.revert(tx)?;
            }
            Ok(())
        })
        .await?;
        self.queue_rollback_items(tx).await?;
        Ok(())
    }

    async fn rebase_local(
        &mut self,
        changeset: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        // sigh
        let mut this = std::mem::replace(
            self,
            ActionMoveData {
                destination: None,
                entries: Default::default(),
            },
        );
        let changeset = changeset.clone();
        let this = tx
            .sync_bridge(move |tx| {
                this.rebase_local_sync(&changeset, tx)
                    .context("Could not rebase local")?;
                Ok(this)
            })
            .await?;
        *self = this;
        Ok(())
    }
    fn rebase_local_sync(
        &mut self,
        changeset: &RebaseChangeSet,
        tx: &Transaction<'_>,
    ) -> Result<(), MailActionError> {
        let ctx = ActionMoveDataV2Ctx::new(tx)?;
        for (id, data) in &mut self.entries {
            let rebase_key: RebaseKey = (*id).into();
            if changeset.contains(&rebase_key) {
                data.rebase::<T>(&ctx, self.destination, *id, tx)?;
            }
        }

        Ok(())
    }

    async fn queue_rollback_items(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        let ids = self.entries.keys().cloned().collect();
        let ids = T::local_ids_counterpart(ids, tx).await?;
        RollbackItem::save_many(tx, ids, T::ROLLBACK_ITEM_TYPE).await?;
        Ok(())
    }

    pub fn action_dependency_keys(&self) -> ActionDependencyKeys {
        let mut keys = ActionDependencyKeysBuilder::default();

        if let Some(destination) = self.destination {
            keys = keys.with_required_related(destination);
        }

        for (id, data) in &self.entries {
            // We could also potentially have several moves interlinked
            // as a dependency where a move chain gets undoed, but it should
            // be okay to have the conversation move to the last operation that succeeded.
            keys = keys
                .with_required_related_many(data.original_locations.clone())
                .with_optional_related(*id)
                // if there is a label as, we should execute after that
                .with_optional_many(label_as_action_dependency_key([*id]));
        }

        keys.build()
    }

    pub fn build_undo_states(&self) -> (messages::LabelAs, messages::Unread) {
        let mut label_as = LabelAsData {
            source_label_id: 0.into(), // This is fine because it's unused (no archiving, no undoing)
            add: vec![],
            remove: vec![],
        };
        let mut mark_unread = Vec::new();

        for data in self.entries.values() {
            mark_unread.extend(data.marked_read.iter().copied());

            for pair in &data.removed_labels {
                label_as.add.push(*pair);
            }
            for pair in &data.applied_labels {
                label_as.remove.push(*pair);
            }
        }

        (
            messages::LabelAs(label_as),
            messages::Unread::new(mark_unread),
        )
    }
}

/// Action which moves target items between two labels.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActionMoveDataV1<T>
where
    T: ConversationOrMessage,
{
    sources: HashMap<Option<LocalLabelId>, Vec<T::IdType>>,
    destination: Option<LocalLabelId>,

    // These 2 exist solely for the revert and undo
    marked_read: Vec<LocalMessageId>,
    removed_labels: Vec<LabelPair<T::IdType>>,
}

#[allow(async_fn_in_trait, reason = "not used across threads")]
pub trait ConversationOrMessage:
    ModelIdExtension<
        IdType: Copy + Hash + Eq + DeserializeOwned + Serialize + LocalIdActionDepExt + From<u64>,
        RemoteId: Display,
    >
{
    const ROLLBACK_ITEM_TYPE: RollbackItemType;

    // -- MAIN DEFS

    fn apply_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        tx: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError>;

    fn remove_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        bond: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError>;

    /// Returns the messages that actually were marked as read
    fn mark_read(
        ids: impl IntoIterator<Item = Self::IdType>,
        tx: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError>;

    // -- HELPER DEFS

    fn get_exclusive_locations(&self) -> Vec<LocalLabelId>;
    fn grouped_labels_and_messages_query(placeholders: usize) -> String;

    // -- API DEFS

    /// If the request succeeds, returns the list of failed ids for which this operation
    /// may have failed.
    async fn api_apply_label(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Result<Vec<Self::RemoteId>, ApiServiceError>;

    /// If the request succeeds, returns the list of failed ids for which this operation
    /// may have failed.
    async fn api_remove_label(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Result<Vec<Self::RemoteId>, ApiServiceError>;

    // -- PROVIDED SHARED IMPLS
    // Most of the actual generic impls are on generics over `ConversationOrMessage`, not in
    // the trait per se.

    // Returns the items that were removed
    fn remove_all_removable_labels(
        ids: &[Self::IdType],
        bond: &Transaction<'_>,
    ) -> Result<Vec<LabelPair<LocalMessageId>>, StashError> {
        let non_removable_labels = {
            let non_removable_labels = LabelId::non_removable_system_labels();
            let mut result = HashSet::with_capacity(non_removable_labels.len());
            for label_id in LabelId::non_removable_system_labels() {
                // In tests these labels may not be defined.
                if let Some(local_id) = Label::remote_id_counterpart_sync(&label_id, bond)? {
                    result.insert(local_id);
                }
            }
            result
        };

        // Not prepare cached because the query depends on the len (it has placeholders)
        let mut stmt = bond.prepare(&Self::grouped_labels_and_messages_query(ids.len()))?;

        let rows = stmt.query_map(params_from_iter(ids), |r| {
            Ok((r.get(0)?, r.get::<_, String>(1)?))
        })?;

        let mut labels_and_messages: Vec<(LocalLabelId, Vec<Self::IdType>)> = vec![];
        for row in rows {
            let (label, ser_ids) = row?;
            if non_removable_labels.contains(&label) {
                continue;
            }

            let mut parsed_ids = vec![];
            for i in ser_ids.split(',') {
                parsed_ids.push(i.parse::<u64>().context("sqlite returned bad data")?.into());
            }
            labels_and_messages.push((label, parsed_ids));
        }

        let mut res = vec![];
        for (label_id, parsed_ids) in labels_and_messages {
            let modified_messages = Self::remove_label(label_id, parsed_ids.iter().copied(), bond)?;

            res.extend(modified_messages.iter().map(|&id| LabelPair {
                id,
                label: label_id,
            }));
        }
        Ok(res)
    }

    // -- Provided async versions

    async fn apply_label_async(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType> + Send + 'static,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        bond.sync_bridge(move |tx| Self::apply_label(local_label_id, ids, tx))
            .await
    }

    async fn remove_label_async(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType> + Send + 'static,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        bond.sync_bridge(move |tx| Self::remove_label(local_label_id, ids, tx))
            .await
    }

    async fn mark_read_async(
        ids: impl IntoIterator<Item = Self::IdType>,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let ids = Vec::from_iter(ids);
        bond.sync_bridge(|tx| Self::mark_read(ids, tx)).await
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelPair<T> {
    pub label: LocalLabelId,
    pub id: T,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAsData<T: ConversationOrMessage> {
    source_label_id: LocalLabelId,
    add: Vec<LabelPair<T::IdType>>,
    remove: Vec<LabelPair<T::IdType>>,
}

impl<T: ConversationOrMessage> LabelAsData<T> {
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

    pub fn new_remove(remove: Vec<LabelPair<T::IdType>>) -> Self {
        Self {
            remove,
            add: vec![],
            source_label_id: 0.into(),
        }
    }

    pub fn new_add(add: Vec<LabelPair<T::IdType>>) -> Self {
        Self {
            remove: vec![],
            add,
            source_label_id: 0.into(),
        }
    }

    async fn apply_local_common(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        let (add, remove) = self.segregate_label();

        tx.sync_bridge(|tx| {
            for (label, ids) in add {
                T::apply_label(label, ids, tx)?;
            }

            for (label, ids) in remove {
                T::remove_label(label, ids, tx)?;
            }
            Ok(())
        })
        .await
    }

    async fn revert_local(&mut self, tx: &Bond<'_>) -> Result<(), StashError> {
        let (add, remove) = self.segregate_label();

        tx.sync_bridge(|tx| {
            for (label, ids) in add {
                T::remove_label(label, ids, tx)?;
            }

            for (label, ids) in remove {
                T::apply_label(label, ids, tx)?;
            }
            Ok(())
        })
        .await
    }

    async fn apply_remote(
        &self,
        api: &Session,
        mut guard: WriterGuard<'_>,
    ) -> Result<(), MailActionError> {
        let (add, remove) = self.segregate_label();

        let almost_all_mail_id = LabelId::almost_all_mail();

        for (label, items) in add {
            let label_id = Label::resolve_remote_label_id(label, guard.tether()).await?;

            if label_id == almost_all_mail_id {
                // This does not need to be communicated to the server, but this action is used
                // by other actions to revert local state, so this may appear.
                continue;
            }

            let items = T::local_ids_counterpart(items, guard.tether()).await?;

            let failed_ids = T::api_apply_label(api, items, label_id).await?;
            if !failed_ids.is_empty() {
                guard
                    .tx::<_, _, anyhow::Error>(async move |tx| {
                        RollbackItem::save_many(tx, failed_ids, T::ROLLBACK_ITEM_TYPE).await?;
                        Ok(())
                    })
                    .await?;
            }
        }

        for (label, items) in remove {
            let label_id = Label::resolve_remote_label_id(label, guard.tether()).await?;
            let items = T::local_ids_counterpart(items, guard.tether()).await?;

            if label_id == almost_all_mail_id {
                // This does not need to be communicated to the server, but this action is used
                // by other actions to revert local state, so this may appear.
                continue;
            }

            let failed_ids = T::api_remove_label(api, items, label_id).await?;
            if !failed_ids.is_empty() {
                guard
                    .tx::<_, _, anyhow::Error>(async move |tx| {
                        RollbackItem::save_many(tx, failed_ids, T::ROLLBACK_ITEM_TYPE).await?;
                        Ok(())
                    })
                    .await?;
            }
        }

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

    fn action_dependency_keys(&self) -> ActionDependencyKeys {
        let mut builder = ActionDependencyKeysBuilder::new();
        let (add, remove) = self.segregate_label();

        for (label, ids) in add.iter().chain(remove.iter()) {
            builder = builder
                .with_required_related(*label)
                .with_optional_many_ext(ids.iter().copied())
                .with_required_many(label_as_action_dependency_key(ids.iter().copied()))
        }

        builder.build()
    }

    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }

    pub fn convert(old_version: u32, data: &[u8]) -> action::FactoryResult<Self> {
        #[allow(dead_code)]
        #[derive(Deserialize)]
        struct OldAction<T: ConversationOrMessage> {
            target_ids: Vec<T::IdType>,
            added_labels: HashMap<T::IdType, HashSet<LocalLabelId>>,
            removed_labels: HashMap<T::IdType, HashSet<LocalLabelId>>,
            source_label_id: LocalLabelId,
        }

        match old_version {
            1 => {
                let data = proton_action_queue::action::deserialize::<OldAction<T>>(data)?;

                let add = data
                    .added_labels
                    .into_iter()
                    .flat_map(|(id, labels)| {
                        labels.into_iter().map(move |label| LabelPair { label, id })
                    })
                    .collect();
                let remove = data
                    .removed_labels
                    .into_iter()
                    .flat_map(|(id, labels)| {
                        labels.into_iter().map(move |label| LabelPair { label, id })
                    })
                    .collect();

                Ok(Self {
                    source_label_id: data.source_label_id,
                    add,
                    remove,
                })
            }
            2 => Ok(proton_action_queue::action::deserialize::<Self>(data)?),
            other_version => Err(FactoryError::InvalidVersion(other_version)),
        }
    }
}

pub enum Undo {
    MessagesLabelAs(UndoLabelAsMessages),
    MessagesMoveTo(UndoMoveToMessages),
    ConversationsLabelAs(UndoLabelAsConversations),
    ConversationsMoveTo(UndoMoveToConversations),
}

impl Undo {
    pub async fn undo(self, queue: &Queue, tether: &mut Tether) -> Result<(), AppError> {
        tracing::info!("undoing!");
        match self {
            Undo::MessagesLabelAs(u) => u.undo(queue, tether).await,
            Undo::ConversationsLabelAs(u) => u.undo(queue, tether).await,
            Undo::MessagesMoveTo(u) => u.undo(queue, tether).await,
            Undo::ConversationsMoveTo(u) => u.undo(queue, tether).await,
        }
    }

    pub fn action_id(&self) -> ActionId {
        match self {
            Undo::MessagesLabelAs(v) => v.id,
            Undo::MessagesMoveTo(v) => v.id,
            Undo::ConversationsLabelAs(v) => v.id,
            Undo::ConversationsMoveTo(v) => v.id,
        }
    }
}

pub struct LabelAsOutput {
    pub input_label_is_empty: bool,
    pub undo: Option<Undo>,
}

fn mark_read_unread_action_dependency_key<T: LocalIdActionDepExt>(
    ids: impl IntoIterator<Item = T>,
) -> impl IntoIterator<Item = ActionDependencyKey> {
    ids.into_iter()
        .map(|id| id.to_custom_dependency_key("mail-mark-read-unread"))
}

fn snooze_unsnooze_action_dependency_key<T: LocalIdActionDepExt>(
    ids: impl IntoIterator<Item = T>,
) -> impl IntoIterator<Item = ActionDependencyKey> {
    ids.into_iter()
        .map(|id| id.to_custom_dependency_key("mail-snooze-unsnooze"))
}

fn label_as_action_dependency_key<T: LocalIdActionDepExt>(
    ids: impl IntoIterator<Item = T>,
) -> impl IntoIterator<Item = ActionDependencyKey> {
    ids.into_iter()
        .map(|id| id.to_custom_dependency_key("mail-label-as"))
}
