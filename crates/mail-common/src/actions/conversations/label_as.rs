use crate::actions::{filter_responses, ActionError, LabelAsData};
use crate::datatypes::SystemLabelId;
use crate::models::{Conversation, ConversationLabel, Label};
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
use tracing::{error, warn};

/// Action to change the labels of a group of conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAs(LabelAsData<Conversation>);

impl LabelAs {
    pub fn new(
        source_label_id: LocalId,
        conversation_ids: Vec<LocalId>,
        selected_label_ids: Vec<LocalId>,
        partially_selected_label_ids: Vec<LocalId>,
        must_archive: bool,
    ) -> Self {
        Self(LabelAsData::new(
            source_label_id,
            conversation_ids,
            selected_label_ids,
            partially_selected_label_ids,
            must_archive,
        ))
    }

    /// Save status of target conversations.
    async fn memorize_original_data<A>(&mut self, interface: &A) -> Result<(), ActionError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for conversation_id in &self.0.local_ids {
            let Some(conversation) = Conversation::load(*conversation_id, interface).await? else {
                warn!("Couldn't find conversation with id: {conversation_id:?}");
                continue;
            };
            self.0.original_labels.insert(
                *conversation_id,
                conversation
                    .labels
                    .iter()
                    .map(|l| l.local_id.expect("Should be set"))
                    .collect(),
            );
            self.0
                .original_locations
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
    type Output = bool;
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler;

impl ActionHandler for Handler {
    type Action = LabelAs;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_remote_ids(tx).await?;
        action.memorize_original_data(tx).await?;

        Conversation::label_as(
            action.0.source_label_id,
            action.0.local_ids.clone(),
            &action.0.local_selected_label_ids,
            &action.0.local_partially_selected_label_ids,
            action.0.must_archive,
            tx,
        )
        .await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let archive_id = RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), tx)
            .await?
            .expect("Archive label must have a RemoteId");

        for conversation_id in &action.0.local_ids {
            let Some(mut conversation) = Conversation::load(*conversation_id, tx).await? else {
                warn!("While reverting locally, could not find conversation with local_id: {conversation_id:?}");
                continue;
            };

            if let Some(labels) = action.0.original_labels.remove(conversation_id) {
                conversation.labels = Vec::with_capacity(labels.len());
                for label_id in labels {
                    if let Some(label) = ConversationLabel::load(label_id, tx).await? {
                        conversation.labels.push(label);
                    } else {
                        warn!("While reverting locally, could not find ConversationLabel with local_id: {label_id:?}");
                    }
                }
            }
            if let Some(location) = action.0.original_locations.get(conversation_id) {
                conversation.exclusive_location = location.clone();
            }
            if action.0.must_archive {
                Conversation::move_conversations(
                    archive_id,
                    action.0.source_label_id,
                    action.0.local_ids.clone(),
                    tx,
                )
                .await?;
            }
            conversation.save_using(tx).await?
        }
        Ok(())
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        let api = session.api();

        let failed_ids = Conversation::remote_relabel(
            session,
            action.0.remote_ids.clone(),
            &action.0.remote_selected_label_ids,
            &action.0.remote_partially_selected_label_ids,
            stash,
        )
        .await?;

        if !failed_ids.is_empty() {
            error!("LabelAs conversation operation failed for conversations: {failed_ids:?}");
            todo!("revert locally for failed ids");
        }

        if action.0.must_archive {
            let conversation_ids = action.0.remote_ids.clone().into_iter().map_into().collect();
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
                    action.0.source_label_id,
                    local_ids,
                    &tx,
                )
                .await?;
                tx.commit().await?;
            }
        }

        if let Some(source_label) = Label::load(action.0.source_label_id, stash).await? {
            Ok(source_label.total_conv == 0)
        } else {
            warn!("Could not find label with id: {}", action.0.source_label_id);
            Ok(true)
        }
    }
}
