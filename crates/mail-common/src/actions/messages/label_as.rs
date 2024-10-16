use crate::actions::{filter_responses, ActionError};
use crate::datatypes::{ExclusiveLocation, SystemLabelId};
use crate::models::{Label, Message};
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
use std::collections::{HashMap, HashSet};
use tracing::{error, warn};

/// Action which change the labels of a messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAs {
    source_label_id: LocalId,
    local_message_ids: Vec<LocalId>,
    remote_message_ids: Vec<RemoteId>,
    local_selected_label_ids: Vec<LocalId>,
    remote_selected_label_ids: HashSet<RemoteId>,
    local_partially_selected_label_ids: Vec<LocalId>,
    remote_partially_selected_label_ids: HashSet<RemoteId>,
    original_labels: HashMap<LocalId, Vec<LabelId>>,
    original_locations: HashMap<LocalId, Option<ExclusiveLocation>>,
    must_archive: bool,
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
            source_label_id,
            local_message_ids: message_ids,
            remote_message_ids: vec![],
            local_selected_label_ids: selected_label_ids,
            remote_selected_label_ids: HashSet::new(),
            local_partially_selected_label_ids: partially_selected_label_ids,
            remote_partially_selected_label_ids: HashSet::new(),
            original_labels: HashMap::new(),
            original_locations: HashMap::new(),
            must_archive,
        }
    }

    async fn resolve_remote_ids(&mut self, tx: &Tether) -> Result<(), ActionError> {
        self.remote_message_ids =
            LocalId::counterparts::<Message, _>(self.local_message_ids.clone(), tx).await?;
        let remote_selected_label_ids =
            LocalId::counterparts::<Label, _>(self.local_selected_label_ids.clone(), tx).await?;
        self.remote_selected_label_ids = remote_selected_label_ids.into_iter().map_into().collect();
        let remote_partially_selected_label_ids =
            LocalId::counterparts::<Label, _>(self.local_partially_selected_label_ids.clone(), tx)
                .await?;
        self.remote_partially_selected_label_ids = remote_partially_selected_label_ids
            .into_iter()
            .map_into()
            .collect();
        Ok(())
    }

    async fn memorize_original_data<A>(&mut self, interface: &A) -> Result<(), ActionError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for message_id in &self.local_message_ids {
            let Some(message) = Message::load(*message_id, interface).await? else {
                warn!("While memorizing labels, could not find message with id: {message_id:?}");
                continue;
            };

            self.original_labels.insert(*message_id, message.label_ids);
            self.original_locations
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
        action.resolve_remote_ids(tx).await?;
        action.memorize_original_data(tx).await?;

        Message::label_messages_as(
            action.source_label_id,
            action.local_message_ids.clone(),
            &action.local_selected_label_ids,
            &action.local_partially_selected_label_ids,
            action.must_archive,
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
        for message_id in &action.local_message_ids {
            let Some(mut message) = Message::load(*message_id, tx).await? else {
                warn!("While reverting locally, could not find message with id: {message_id:?}");
                continue;
            };

            if let Some(labels) = action.original_labels.remove(message_id) {
                message.label_ids = labels;
            }
            if let Some(location) = action.original_locations.get(message_id) {
                message.exclusive_location = location.clone();
            }
            if action.must_archive {
                let archive_id =
                    RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), tx)
                        .await?
                        .expect("Archive label must have a RemoteId");
                Message::move_messages(
                    archive_id,
                    action.source_label_id,
                    action.local_message_ids.clone(),
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
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let api = session.api();

        for message_id in &action.local_message_ids {
            let Some(message) = Message::load(*message_id, stash).await? else {
                warn!("While labeling messages, could not find message with id: {message_id:?}");
                continue;
            };

            message
                .relabel_message(
                    session,
                    &action.local_selected_label_ids,
                    &action.local_partially_selected_label_ids,
                    stash,
                )
                .await?;
        }

        if action.must_archive {
            let message_ids = action
                .remote_message_ids
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
                Message::move_messages(archive_id, action.source_label_id, local_ids, &tx).await?;
                tx.commit().await?;
            }
        }

        if let Some(source_label) = Label::load(action.source_label_id, stash).await? {
            Ok(source_label.total_msg == 0)
        } else {
            warn!("Could not find label with id: {}", action.source_label_id);
            Ok(true)
        }
    }
}
