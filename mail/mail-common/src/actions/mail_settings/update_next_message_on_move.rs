use crate::actions::MailActionError;
use crate::datatypes::NextMessageOnMove;
use crate::models::MailSettings;
use anyhow::Context;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::session::Session;
use proton_mail_api::services::proton::{ProtonMail, request_data::PutNextMessageOnMoveRequest};
use serde::{Deserialize, Serialize};
use stash::UserDb;
use stash::orm::Model;
use stash::stash::{Bond, RunTransaction};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateNextMessageOnMove {
    pub next_message_on_move: bool,
    old_next_message_on_move: Option<NextMessageOnMove>,
}

impl UpdateNextMessageOnMove {
    pub fn new(next_message_on_move: bool) -> Self {
        Self {
            next_message_on_move,
            old_next_message_on_move: None,
        }
    }
}

impl Action<UserDb> for UpdateNextMessageOnMove {
    const TYPE: Type = Type("update_next_message_on_move");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UpdateNextMessageOnMoveHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct UpdateNextMessageOnMoveHandler {
    pub api: Session,
}

impl Handler<UserDb> for UpdateNextMessageOnMoveHandler {
    type Action = UpdateNextMessageOnMove;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let mut mail_settings = match MailSettings::get(bond.tether()).await? {
            Some(ms) => ms,
            None => {
                tracing::warn!("Failed to get mail settings");
                MailSettings::default()
            }
        };

        action.old_next_message_on_move = mail_settings.next_message_on_move;
        mail_settings.next_message_on_move = Some(if action.next_message_on_move {
            NextMessageOnMove::EnabledExplicit
        } else {
            NextMessageOnMove::DisabledExplicit
        });

        mail_settings.save(bond).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let mut mail_settings = match MailSettings::get(bond.tether()).await? {
            Some(ms) => ms,
            None => {
                tracing::warn!("Failed to get mail settings");
                MailSettings::default()
            }
        };
        mail_settings.next_message_on_move = action.old_next_message_on_move;
        mail_settings.save(bond).await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _guard: WriterGuard<'_, UserDb>,
    ) -> Result<
        <Self::Action as Action<UserDb>>::RemoteOutput,
        <Self::Action as Action<UserDb>>::Error,
    > {
        let request = PutNextMessageOnMoveRequest {
            next_message_on_move: action.next_message_on_move,
        };

        let _response = self
            .api
            .put_next_message_on_move(request)
            .await
            .context("Failed to update next message on move setting")?;

        Ok(())
    }
    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        Ok(())
    }
}
