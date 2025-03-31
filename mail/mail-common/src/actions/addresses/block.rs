use crate::MailUserContext;
use crate::actions::MailActionError;
use crate::models::default_location::IncomingDefaultLocation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler as ActionHandler};
use proton_api_mail::services::proton::ProtonMail;
use proton_api_mail::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use proton_core_common::datatypes::LocalAddressId;
use serde::{Deserialize, Serialize};
use stash::params;
use stash::stash::Bond;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
enum BlockOrUnblock {
    Block,
    Unblock,
}

/// Action which blocks or unblocks an address
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Block {
    address: LocalAddressId,
    action: BlockOrUnblock,
}

impl Block {
    #[allow(clippy::self_named_constructors)]
    /// Create a new instance which blocks the address
    pub fn block(address: LocalAddressId) -> Self {
        Self {
            address,
            action: BlockOrUnblock::Block,
        }
    }

    /// Create a new instance which unblocks the address
    pub fn unblock(address: LocalAddressId) -> Self {
        Self {
            address,
            action: BlockOrUnblock::Unblock,
        }
    }
}

impl Action for Block {
    const TYPE: Type = Type("(un)block");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = MailActionError;

    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl ActionHandler for Handler {
    type Action = Block;

    type Context = MailUserContext;
    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if action.action == BlockOrUnblock::Block {
            IncomingDefaultLocation::store_by_id(
                action.address,
                IncomingDefaultLocation::Blocked,
                bond,
            )
            .await?;
        } else {
            IncomingDefaultLocation::delete_by_id(action.address, bond).await?;
        }
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if action.action == BlockOrUnblock::Block {
            IncomingDefaultLocation::delete_by_id(action.address, bond).await?;
        } else {
            IncomingDefaultLocation::store_by_id(
                action.address,
                IncomingDefaultLocation::Blocked,
                bond,
            )
            .await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let tether = guard.tether();
        let email = tether
            .query_value::<_, String>(
                "SELECT email AS value FROM addresses WHERE local_id = ?",
                params![action.address],
            )
            .await?;
        if action.action == BlockOrUnblock::Block {
            ctx.api()
                .post_incoming_default(ApiIncomingDefaultLocation::Blocked, &email)
                .await?;
        } else {
            ctx.api()
                .update_incoming_default(ApiIncomingDefaultLocation::Inbox, &email)
                .await?;
        }

        Ok(())
    }
}
