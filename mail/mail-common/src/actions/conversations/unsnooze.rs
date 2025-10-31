use crate::actions::{GenericLabelRelatedActionData, MailActionError, filter_responses};
use crate::datatypes::{LocalConversationId, RollbackItemType};
use crate::models::{Conversation, ConversationLabel, RollbackItem};
use itertools::Itertools;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_core_api::session::Session;
use proton_core_common::datatypes::{LocalLabelId, SystemLabel, UnixTimestamp};
use proton_mail_api::services::proton::ProtonMail;
use serde::{self, Deserialize, Serialize};
use stash::exports::ToSql;
use stash::orm::Model;
use stash::stash::Bond;
use stash::utils::placeholders;
use tracing::error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unsnooze {
    action_data: GenericLabelRelatedActionData<Conversation>,
    conv_snooze_time: Vec<(LocalConversationId, UnixTimestamp)>,
}

impl Unsnooze {
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
        Self {
            action_data: GenericLabelRelatedActionData::new(label_id, ids),
            conv_snooze_time: vec![],
        }
    }
}

impl Action for Unsnooze {
    const TYPE: Type = Type("unsnooze_conversations");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UnsnoozeHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.action_data
            .snooze_unsnooze_action_dependency_keys()
            .build()
    }
}

pub struct UnsnoozeHandler {
    pub api: Session,
}

impl Handler for UnsnoozeHandler {
    type Action = Unsnooze;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        if action.action_data.data.target_ids.is_empty() {
            return Err(MailActionError::NoInput);
        }

        let mut parameters = action
            .action_data
            .data
            .target_ids
            .iter()
            .copied()
            .map(|i| Box::new(i) as Box<dyn ToSql + Send>)
            .collect_vec();
        let placeholders = placeholders(&parameters);
        parameters.push(Box::new(SystemLabel::Snoozed.remote_id()));
        let conv_labels = ConversationLabel::find(
            format!("WHERE local_conversation_id IN ({placeholders}) AND remote_label_id=?"),
            parameters,
            tx,
        )
        .await?;

        for conv_label in conv_labels {
            action.conv_snooze_time.push((
                conv_label
                    .local_conversation_id
                    .expect("Conversation label must have a conversation id"),
                conv_label.context_snooze_time,
            ));
        }

        Conversation::unsnooze(
            action.action_data.label_id,
            &action.action_data.data.target_ids,
            tx,
        )
        .await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        for (conv_id, snoozed_until) in action.conv_snooze_time.iter() {
            Conversation::snooze(action.action_data.label_id, &[*conv_id], *snoozed_until, tx)
                .await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        action.action_data.resolve_ids(guard.tether()).await?;

        if action.action_data.data.remote_target_ids.is_empty() {
            tracing::warn!(
                "No remote target ids to unsnooze, local only ids: {:?}",
                action.action_data.data.target_ids
            );
            return Ok(());
        }

        let response = self
            .api
            .put_conversations_unsnooze(action.action_data.data.remote_target_ids.clone())
            .await?;

        let responses = filter_responses(response.responses);

        if !responses.is_empty() {
            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    error!("Unsnooze operation failed for: {:?}", responses);

                    for remote_id in responses {
                        RollbackItem::new(remote_id.to_string(), RollbackItemType::Conversation)
                            .save(tx)
                            .await?;
                    }

                    Ok(())
                })
                .await?;
        }

        Ok(())
    }

    async fn rebase_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        //TODO(ET-5183): Test me!
        self.apply_local(this_id, action, tx).await?;
        Ok(())
    }
}
