use crate::actions::MailActionError;
use crate::actions::addresses::incoming_defaults_dependency_key;
use crate::datatypes::LocalIncomingDefaultId;
use crate::models::{IncomingDefault, IncomingDefaultLocation};
use anyhow::anyhow;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, DefaultVersionConverter, Type, WriterGuard,
};
use proton_action_queue::action::{ActionId, Handler};
use proton_core_api::services::proton::PrivateEmail;
use proton_core_api::session::Session;
use proton_core_common::actions::dependency_builder::ActionDependencyKeysBuilder;
use proton_core_common::models::ModelExtension;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Block {
    pub email: PrivateEmail,
    local_id: Option<LocalIncomingDefaultId>,
}

impl Block {
    pub fn new(email: PrivateEmail) -> Self {
        Self {
            email,
            local_id: None,
        }
    }
}

impl Action for Block {
    const TYPE: Type = Type("block");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = BlockHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::new()
            .with_required(incoming_defaults_dependency_key(&self.email))
            .build()
    }
}

pub struct BlockHandler {
    pub api: Session,
}

impl Handler for BlockHandler {
    type Action = Block;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        tracing::info!("Blocking {}", action.email);

        let mut incoming_default =
            IncomingDefault::by_email(action.email.as_clear_text_str(), bond)
                .await?
                .unwrap_or_else(|| IncomingDefault {
                    local_id: None,
                    remote_id: None,
                    email: action.email.clone(),
                    domain: None,
                    location: IncomingDefaultLocation::Blocked,
                    deleted: false,
                });
        incoming_default.save(bond).await?;
        action.local_id = incoming_default.local_id;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        tracing::info!(
            "Removing block for {} ({:?})",
            action.email,
            action.local_id
        );

        let Some(local_id) = action.local_id else {
            return Err(anyhow!("Missing local_id").into());
        };

        IncomingDefault::delete_by_id(local_id, bond).await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        tracing::info!("Blocking {}", action.email);

        let Some(local_id) = action.local_id else {
            return Err(anyhow!("Missing local_id").into());
        };

        let new_incoming = self
            .api
            .post_incoming_default(ApiIncomingDefaultLocation::Blocked, &action.email)
            .await?
            .incoming_default;

        guard
            .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                IncomingDefault::update_from_api(local_id, new_incoming, tx).await?;
                Ok(())
            })
            .await?;

        Ok(())
    }
}
