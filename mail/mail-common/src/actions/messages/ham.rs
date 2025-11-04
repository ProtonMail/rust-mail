use crate::actions::MailActionError;
use crate::datatypes::{LocalMessageId, MessageFlags, RollbackItemType};
use crate::models::{Message, RollbackItem};
use futures::future::try_join_all;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, DefaultVersionConverter, Type, WriterGuard,
};
use proton_action_queue::action::{ActionId, Handler};
use proton_core_api::session::Session;
use proton_core_common::actions::dependency_builder::ActionDependencyKeysBuilder;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::info;

/// Action which marks messages as Ham.
/// Ham means that a message is not spam (get it?)
/// This also applies to messages thought to be phishing or suspicious.
///
/// It will also whitelist the sender
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Ham(Vec<LocalMessageId>);

impl Ham {
    /// Create a new instance which marks the messages as deleted.
    /// This should only be called for messages that are in spam.
    pub fn new(message_ids: Vec<LocalMessageId>) -> Self {
        Self(message_ids)
    }
}

impl Action for Ham {
    const TYPE: Type = Type("mark_ham");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = HamHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::new()
            .with_optional_many_ext(self.0.iter().copied())
            .build()
    }
}

pub struct HamHandler {
    pub api: Session,
}

impl Handler for HamHandler {
    type Action = Ham;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if action.0.is_empty() {
            return Err(MailActionError::NoInput);
        }

        for &id in &action.0 {
            Message::set_flags(id, MessageFlags::HAM_MANUAL, bond).await?;
        }

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        for &id in &action.0 {
            Message::unset_flags(id, MessageFlags::HAM_MANUAL, bond).await?;
        }

        let ids = Message::local_ids_counterpart(action.0.clone(), bond).await?;

        for id in ids {
            RollbackItem::new(id.to_string(), RollbackItemType::Message)
                .save(bond)
                .await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let tether = guard.tether();
        let ids = Message::local_ids_counterpart(action.0.clone(), tether).await?;

        info!("Marking {ids:?} as not spam");

        let iter = ids.iter().map(|id| self.api.put_message_ham(id));

        _ = try_join_all(iter).await?;

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
