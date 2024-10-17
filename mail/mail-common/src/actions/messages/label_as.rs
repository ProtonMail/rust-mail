use crate::actions::{filter_responses, ActionError, LabelAsData};
use crate::datatypes::SystemLabelId;
use crate::models::{Label, Message};
use crate::MailUserContext;
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

/// Action which change the labels of a messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAs {
    data: LabelAsData<Message>,
}

impl LabelAs {
    pub fn new(
        source_label_id: LocalId,
        message_ids: Vec<LocalId>,
        selected_label_ids: Vec<LocalId>,
        partially_selected_label_ids: Vec<LocalId>,
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

    /// Save status of target conversations.
    async fn memorize_original_data<A>(&mut self, interface: &A) -> Result<(), ActionError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        self.data.memorize_original_data(interface).await?;
        for message_id in &self.data.local_ids {
            let Some(message) = Message::load(*message_id, interface).await? else {
                warn!("While memorizing labels, could not find message with id: {message_id:?}");
                continue;
            };

            self.data
                .original_location
                .insert(*message_id, message.exclusive_location);
        }
        Ok(())
    }
}

impl Action for LabelAs {
    const TYPE: Type = Type("label_messages_as");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = bool;

    type LocalOutput = ();
    type Error = ActionError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl ActionHandler for Handler {
    type Action = LabelAs;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
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

        Ok(())
    }

    async fn revert_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let archive_id = RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), tx)
            .await?
            .expect("Archive label must have a RemoteId");

        for message_id in &action.data.local_ids {
            let Some(mut message) = Message::load(*message_id, tx).await? else {
                warn!("While reverting locally, could not find message with id: {message_id:?}");
                continue;
            };

            let added_labels = action
                .data
                .added_labels
                .remove(message_id)
                .unwrap_or_default();
            let removed_labels = action
                .data
                .removed_labels
                .remove(message_id)
                .unwrap_or_default();
            let current_labels = RemoteId::counterparts::<Label, _>(
                message
                    .label_ids
                    .iter()
                    .map(|l| l.clone().into_inner())
                    .collect(),
                tx,
            )
            .await?;
            let current_labels = HashSet::from_iter(current_labels.into_iter());
            let new_labels = &(&current_labels - &removed_labels) | &added_labels;
            let new_labels =
                LocalId::counterparts::<Label, _>(Vec::from_iter(new_labels), tx).await?;
            message.label_ids = new_labels.into_iter().map_into().collect();

            if let Some(location) = action.data.original_location.get(message_id) {
                message.exclusive_location = location.clone();
            }
            if action.data.must_archive {
                Message::move_messages(
                    archive_id,
                    action.data.source_label_id,
                    action.data.local_ids.clone(),
                    tx,
                )
                .await?;
            }
            message.save_using(tx).await?
        }
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let api = session.api();

        for message_id in &action.data.local_ids {
            let Some(message) = Message::load(*message_id, stash).await? else {
                warn!("While labeling messages, could not find message with id: {message_id:?}");
                continue;
            };

            message
                .relabel_message(
                    session,
                    &action.data.local_selected_label_ids,
                    &action.data.local_partially_selected_label_ids,
                    stash,
                )
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
                .put_messages_label(message_ids, LabelId::archive().into_inner().into(), None)
                .await?
                .responses;

            let failed_ids = filter_responses(response);
            if !failed_ids.is_empty() {
                error!("Archive messages operation failed for : {failed_ids:?}");

                let tx = stash.transaction().await?;
                let archive_id =
                    RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), &tx)
                        .await?
                        .expect("Archive label must have a RemoteId");
                let local_ids =
                    RemoteId::counterparts::<Message, _>(failed_ids.clone(), &tx).await?;
                Message::move_messages(archive_id, action.data.source_label_id, local_ids, &tx)
                    .await?;
                tx.commit().await?;
            }
        }

        if let Some(source_label) = Label::load(action.data.source_label_id, stash).await? {
            Ok(source_label.total_msg == 0)
        } else {
            warn!(
                "Could not find label with id: {}",
                action.data.source_label_id
            );
            Ok(true)
        }
    }
}
