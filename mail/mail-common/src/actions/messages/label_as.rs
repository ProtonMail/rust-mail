use crate::actions::{filter_responses, ActionError, LabelAsData};
use crate::datatypes::{LabelType, RollbackItemType, SystemLabelId};
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

    /// Memorize the data before applying LabelAs action so we can revert modifications later
    async fn memorize_original_data<A>(&mut self, interface: &A) -> Result<(), ActionError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let all_labels = Label::find_by_kind(LabelType::Label, interface).await?;
        self.data.local_all_label_ids = all_labels
            .iter()
            .map(|l| l.local_id.expect("Should be set"))
            .collect();

        self.save_modifications(interface).await?;

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

    /// Keep track of labels added/removed
    async fn save_modifications<A>(&mut self, interface: &A) -> Result<(), ActionError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let selected = HashSet::from_iter(self.data.local_selected_label_ids.iter().cloned());
        let partial =
            HashSet::from_iter(self.data.local_partially_selected_label_ids.iter().cloned());
        for message_id in &self.data.local_ids {
            if let Some(message) = Message::load(*message_id, interface).await? {
                let labels = message
                    .label_ids
                    .into_iter()
                    .map(|l| l.into_inner())
                    .collect();
                let labels = RemoteId::counterparts::<Label, _>(labels, interface).await?;
                let labels = HashSet::from_iter(labels.into_iter());
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
