use crate::actions::{LabelAsData, MailActionError, filter_responses};
use crate::datatypes::{ExclusiveLocation, LocalMessageId, RollbackItemType, SystemLabelId};
use crate::models::{Message, MessageCounters};
use crate::{AppError, MailUserContext};
use itertools::Itertools;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler as ActionHandler, Type, WriterGuard,
};
use proton_api_core::services::proton::LabelId;
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{LabelType, LocalLabelId};
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, Tether};
use std::collections::HashSet;
use tracing::{error, warn};

/// Action which change the labels of a messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAs {
    data: LabelAsData<Message>,
}

impl LabelAs {
    pub fn new(
        source_label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
        selected_label_ids: Vec<LocalLabelId>,
        partially_selected_label_ids: Vec<LocalLabelId>,
        must_archive: bool,
    ) -> Self {
        Self {
            data: LabelAsData::new(
                source_label_id,
                message_ids,
                selected_label_ids,
                partially_selected_label_ids,
                must_archive,
            ),
        }
    }

    /// Memorize the data before applying LabelAs action so we can revert modifications later
    async fn memorize_original_data(&mut self, tether: &Tether) -> Result<(), MailActionError> {
        let all_labels = Label::find_by_kind(LabelType::Label, tether).await?;
        self.data.local_all_label_ids = all_labels
            .iter()
            .map(|l| l.local_id.expect("Should be set"))
            .collect();

        self.save_modifications(tether).await?;

        for message_id in &self.data.local_ids {
            let Some(message) = Message::load(*message_id, tether).await? else {
                warn!("While memorizing labels, could not find message with id: {message_id:?}");
                continue;
            };

            self.data
                .original_location
                .insert(*message_id, message.exclusive_location);
        }
        Ok(())
    }

    /// Keep track of labels added/removed
    async fn save_modifications(&mut self, tether: &Tether) -> Result<(), MailActionError> {
        let selected = HashSet::from_iter(self.data.local_selected_label_ids.iter().cloned());
        let partial =
            HashSet::from_iter(self.data.local_partially_selected_label_ids.iter().cloned());
        for message_id in &self.data.local_ids {
            if let Some(message) = Message::load(*message_id, tether).await? {
                let labels = message.all_message_labels(tether).await?;
                let labels = labels
                    .iter()
                    .filter(|l| l.label_type == LabelType::Label)
                    .filter_map(|l| l.local_id)
                    .collect();
                self.data
                    .added_labels
                    .insert(*message_id, &selected - &labels);
                self.data
                    .removed_labels
                    .insert(*message_id, &(&labels - &selected) - &partial);
            }
        }
        Ok(())
    }
}

impl Action for LabelAs {
    const TYPE: Type = Type("label_messages_as");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = bool;
    type Error = MailActionError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl Handler {
    pub(crate) async fn revert_one_locally(
        message_id: LocalMessageId,
        added_labels: HashSet<LocalLabelId>,
        removed_labels: HashSet<LocalLabelId>,
        original_locations: Option<Option<ExclusiveLocation>>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let Some(mut message) = Message::load(message_id, bond).await? else {
            warn!("While reverting locally, could not find message with local_id: {message_id:?}");
            return Ok(());
        };

        let current_labels = message.label_ids.iter().cloned().collect_vec();
        let current_labels: HashSet<_> = HashSet::from_iter(current_labels);
        let removed_labels =
            Label::local_ids_counterpart(Vec::from_iter(removed_labels), bond).await?;
        let removed_labels = HashSet::from_iter(removed_labels);
        let added_labels = Label::local_ids_counterpart(Vec::from_iter(added_labels), bond).await?;
        let added_labels = HashSet::from_iter(added_labels);
        let new_labels = &(&current_labels - &removed_labels) | &added_labels;
        message.label_ids = new_labels.into_iter().map_into().collect();

        if let Some(location) = original_locations {
            message.exclusive_location = location;
        }
        message.save(bond).await?;

        Ok(())
    }
}

impl ActionHandler for Handler {
    type Action = LabelAs;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<bool, <Self::Action as Action>::Error> {
        action.memorize_original_data(tx).await?;
        action.data.resolve_remote_ids(tx).await?;

        Message::label_as(
            action.data.source_label_id,
            action.data.local_ids.clone(),
            &action.data.local_selected_label_ids,
            &action.data.local_partially_selected_label_ids,
            &action.data.local_all_label_ids,
            action.data.must_archive,
            tx,
        )
        .await?;

        if let Some(source_label_counters) =
            MessageCounters::find_by_id(action.data.source_label_id, tx).await?
        {
            Ok(source_label_counters.total == 0)
        } else {
            warn!(
                "Could not find label with id: {}",
                action.data.source_label_id
            );
            Ok(true)
        }
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::undo_label_as(
            action.data.local_ids.clone(),
            action.data.source_label_id,
            action.data.added_labels.clone(),
            action.data.removed_labels.clone(),
            action.data.original_location.clone(),
            action.data.must_archive,
            tx,
        )
        .await?;

        action
            .data
            .mark_rollback(RollbackItemType::Message, tx)
            .await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let session = ctx.session();
        let api = session.api();

        let failed_ids = Message::remote_relabel(
            session,
            &action.data.added_labels,
            &action.data.removed_labels,
            guard.tether(),
        )
        .await?;

        if !failed_ids.is_empty() {
            error!("LabelAs message operation failed for messages: {failed_ids:?}");
            let failed_ids = Message::remote_ids_counterpart(failed_ids, guard.tether()).await?;
            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    for message_id in failed_ids {
                        Self::revert_one_locally(
                            message_id,
                            action
                                .data
                                .added_labels
                                .remove(&message_id)
                                .unwrap_or_default(),
                            action
                                .data
                                .removed_labels
                                .remove(&message_id)
                                .unwrap_or_default(),
                            action.data.original_location.remove(&message_id),
                            tx,
                        )
                        .await?;
                    }
                    Ok(())
                })
                .await?;
        }

        if action.data.must_archive {
            let message_ids = action
                .data
                .remote_ids
                .clone()
                .into_iter()
                .map_into()
                .collect();
            let response = api
                .put_messages_label(message_ids, LabelId::archive(), None)
                .await?
                .responses;

            let failed_ids = filter_responses(response);
            if !failed_ids.is_empty() {
                error!("Archive messages operation failed for : {failed_ids:?}");

                guard
                    .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                        let archive_id = Label::remote_id_counterpart(LabelId::archive(), tx)
                            .await?
                            .expect("Archive label must have a RemoteId");
                        let local_ids =
                            Message::remote_ids_counterpart(failed_ids.clone(), tx).await?;
                        Message::move_messages(
                            archive_id,
                            action.data.source_label_id,
                            local_ids,
                            tx,
                        )
                        .await?;
                        Ok(())
                    })
                    .await?;
            }
        }

        Ok(())
    }
}
