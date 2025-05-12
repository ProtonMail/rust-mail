use crate::actions::{LabelAsData, MailActionError, filter_responses};
use crate::datatypes::{ExclusiveLocation, RollbackItemType, SystemLabelId};
use crate::models::{Conversation, ConversationCounters, ConversationLabel};
use crate::{AppError, MailUserContext};
use itertools::Itertools;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler as ActionHandler, Type, WriterGuard,
};
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::CoreSession;
use proton_core_common::datatypes::{LabelType, LocalLabelId};
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_ids::LocalConversationId;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, Tether};
use std::collections::HashSet;
use tracing::{error, warn};

/// Action to change the labels of a group of conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAs {
    data: LabelAsData<Conversation>,
}

impl LabelAs {
    pub fn new(
        source_label_id: LocalLabelId,
        conversation_ids: Vec<LocalConversationId>,
        selected_label_ids: Vec<LocalLabelId>,
        partially_selected_label_ids: Vec<LocalLabelId>,
        must_archive: bool,
    ) -> Self {
        Self {
            data: LabelAsData::new(
                source_label_id,
                conversation_ids,
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
        for conversation_id in &self.data.local_ids {
            let Some(conversation) = Conversation::load(*conversation_id, tether).await? else {
                warn!("Couldn't find conversation with id: {conversation_id:?}");
                continue;
            };
            self.data
                .original_location
                .insert(*conversation_id, conversation.exclusive_location);
        }
        Ok(())
    }

    /// Keep track of labels added/removed
    async fn save_modifications(&mut self, tether: &Tether) -> Result<(), MailActionError> {
        let selected = HashSet::from_iter(self.data.local_selected_label_ids.iter().cloned());
        let partial =
            HashSet::from_iter(self.data.local_partially_selected_label_ids.iter().cloned());
        for local_id in &self.data.local_ids {
            let labels = ConversationLabel::labels_ids_for_conversation(*local_id, tether).await?;
            let labels = Label::find_by_ids(labels, tether).await?;
            let labels = labels
                .iter()
                .filter(|l| l.label_type == LabelType::Label)
                .filter_map(|l| l.local_id)
                .collect();
            self.data
                .added_labels
                .insert(*local_id, &selected - &labels);
            self.data
                .removed_labels
                .insert(*local_id, &(&labels - &selected) - &partial);
        }
        Ok(())
    }
}

impl Action for LabelAs {
    const TYPE: Type = Type("label_conversation_as");
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
        conversation_id: LocalConversationId,
        added_labels: HashSet<LocalLabelId>,
        removed_labels: HashSet<LocalLabelId>,
        original_locations: Option<Option<ExclusiveLocation>>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let Some(mut conversation) = Conversation::load(conversation_id, bond).await? else {
            warn!(
                "While reverting locally, could not find conversation with local_id: {conversation_id:?}"
            );
            return Ok(());
        };

        let current_labels = HashSet::from_iter(
            conversation
                .labels
                .iter()
                .map(|l| l.local_label_id.expect("Should be set")),
        );
        let new_labels = &(&current_labels - &removed_labels) | &added_labels;
        conversation.labels = ConversationLabel::find_by_label_ids(new_labels, bond).await?;

        if let Some(location) = original_locations {
            conversation.exclusive_location = location;
        }
        conversation.save(bond).await?;

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

        Conversation::label_as(
            action.data.source_label_id,
            action.data.local_ids.clone(),
            &action.data.local_selected_label_ids,
            &action.data.local_partially_selected_label_ids,
            action.data.must_archive,
            tx,
        )
        .await?;

        if let Some(source_conv_counter) =
            ConversationCounters::load(action.data.source_label_id, tx).await?
        {
            Ok(source_conv_counter.total == 0)
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
        Conversation::undo_label_as(
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
            .mark_rollback(RollbackItemType::Conversation, tx)
            .await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let session = ctx.session();

        let failed_ids = Conversation::remote_relabel(
            session,
            &action.data.added_labels,
            &action.data.removed_labels,
            guard.tether(),
        )
        .await?;

        if !failed_ids.is_empty() {
            error!("LabelAs conversation operation failed for conversations: {failed_ids:?}");
            let failed_ids =
                Conversation::remote_ids_counterpart(failed_ids, guard.tether()).await?;
            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx: &Bond<'_>| {
                    for conversation_id in failed_ids {
                        Self::revert_one_locally(
                            conversation_id,
                            action
                                .data
                                .added_labels
                                .remove(&conversation_id)
                                .unwrap_or_default(),
                            action
                                .data
                                .removed_labels
                                .remove(&conversation_id)
                                .unwrap_or_default(),
                            action.data.original_location.remove(&conversation_id),
                            tx,
                        )
                        .await?;
                    }
                    Ok(())
                })
                .await?;
        }

        if action.data.must_archive {
            let conversation_ids = action
                .data
                .remote_ids
                .clone()
                .into_iter()
                .map_into()
                .collect();
            let response = session
                .api()
                .put_conversations_label(conversation_ids, LabelId::archive(), None)
                .await?
                .responses;

            let failed_ids = filter_responses(response);
            if !failed_ids.is_empty() {
                error!("Archive conversation operation failed for : {failed_ids:?}");

                guard
                    .tx::<_, _, <Self::Action as Action>::Error>(async |tx: &Bond<'_>| {
                        let archive_id = Label::remote_id_counterpart(LabelId::archive(), tx)
                            .await?
                            .expect("Archive label must have a RemoteId");
                        let local_ids =
                            Conversation::remote_ids_counterpart(failed_ids.clone(), tx).await?;
                        Conversation::move_conversations(
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
