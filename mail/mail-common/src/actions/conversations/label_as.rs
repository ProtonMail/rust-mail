use crate::actions::{filter_responses, ActionError, LabelAsData};
use crate::datatypes::{ExclusiveLocation, SystemLabelId};
use crate::models::{Conversation, ConversationLabel, Label};
use crate::{AppError, MailUserContext};
use itertools::Itertools;
use proton_action_queue::action::{
    Action, DefaultVersionConverter, Handler as ActionHandler, Type,
};
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{AgnosticInterface, Interface, Stash, Tether};
use std::collections::HashSet;
use tracing::{error, warn};

/// Action to change the labels of a group of conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAs {
    data: LabelAsData<Conversation>,
}

impl LabelAs {
    pub fn new(
        source_label_id: LocalId,
        conversation_ids: Vec<LocalId>,
        selected_label_ids: Vec<LocalId>,
        partially_selected_label_ids: Vec<LocalId>,
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

    /// Save status of target conversations.
    async fn memorize_original_data<A>(&mut self, interface: &A) -> Result<(), ActionError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        self.data.memorize_original_data(interface).await?;
        for conversation_id in &self.data.local_ids {
            let Some(conversation) = Conversation::load(*conversation_id, interface).await? else {
                warn!("Couldn't find conversation with id: {conversation_id:?}");
                continue;
            };
            self.data
                .original_location
                .insert(*conversation_id, conversation.exclusive_location);
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
    type Error = ActionError;

    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl Handler {
    pub(crate) async fn revert_one_locally<A>(
        conversation_id: &LocalId,
        added_labels: HashSet<LocalId>,
        removed_labels: HashSet<LocalId>,
        original_locations: Option<Option<ExclusiveLocation>>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(mut conversation) = Conversation::load(*conversation_id, interface).await? else {
            warn!("While reverting locally, could not find conversation with local_id: {conversation_id:?}");
            return Ok(());
        };

        let current_labels = HashSet::from_iter(
            conversation
                .labels
                .iter()
                .map(|l| l.local_id.expect("Should be set")),
        );
        let new_labels = &(&current_labels - &removed_labels) | &added_labels;
        conversation.labels = ConversationLabel::find_by_ids(new_labels, interface).await?;

        if let Some(location) = original_locations {
            conversation.exclusive_location = location;
        }
        conversation.save_using(interface).await?;

        Ok(())
    }
}

impl ActionHandler for Handler {
    type Action = LabelAs;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
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

        if let Some(source_label) = Label::load(action.data.source_label_id, tx).await? {
            Ok(source_label.total_conv == 0)
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
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
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
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let api = session.api();

        let failed_ids = Conversation::remote_relabel(
            session,
            &action.data.added_labels,
            &action.data.removed_labels,
            stash,
        )
        .await?;

        if !failed_ids.is_empty() {
            error!("LabelAs conversation operation failed for conversations: {failed_ids:?}");
            let failed_ids = RemoteId::counterparts::<Conversation, _>(failed_ids, stash).await?;
            for conversation_id in &failed_ids {
                Self::revert_one_locally(
                    conversation_id,
                    action
                        .data
                        .added_labels
                        .remove(conversation_id)
                        .unwrap_or_default(),
                    action
                        .data
                        .removed_labels
                        .remove(conversation_id)
                        .unwrap_or_default(),
                    action.data.original_location.remove(conversation_id),
                    stash,
                )
                .await?;
            }
        }

        if action.data.must_archive {
            let conversation_ids = action
                .data
                .remote_ids
                .clone()
                .into_iter()
                .map_into()
                .collect();
            let response = api
                .put_conversations_label(
                    conversation_ids,
                    LabelId::archive().into_inner().into(),
                    None,
                )
                .await?
                .responses;

            let failed_ids = filter_responses(response);
            if !failed_ids.is_empty() {
                error!("Archive conversation operation failed for : {failed_ids:?}");

                let tx = stash.transaction().await?;
                let archive_id =
                    RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), &tx)
                        .await?
                        .expect("Archive label must have a RemoteId");
                let local_ids =
                    RemoteId::counterparts::<Conversation, _>(failed_ids.clone(), &tx).await?;
                Conversation::move_conversations(
                    archive_id,
                    action.data.source_label_id,
                    local_ids,
                    &tx,
                )
                .await?;
                tx.commit().await?;
            }
        }
        Ok(())
    }
}
