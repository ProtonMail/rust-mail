use crate::models::Label;
use crate::{actions::ActionError, models::ModelError};
use anyhow::anyhow;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::LabelId;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Tether;
use tracing::{debug, error};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Expand {
    local_id: u64,
    remote_id: Option<LabelId>,
    expand: bool,
    original_state: Option<bool>,
    remote_failed: bool,
}

impl Expand {
    #[allow(clippy::self_named_constructors)]
    pub fn expand(local_id: u64) -> Self {
        Self::new(local_id, true)
    }

    pub fn collapse(local_id: u64) -> Self {
        Self::new(local_id, false)
    }

    fn new(local_id: u64, expand: bool) -> Self {
        Self {
            local_id,
            expand,
            remote_id: None,
            remote_failed: false,
            original_state: None,
        }
    }

    fn is_applicable(&self) -> bool {
        self.original_state.is_some() && self.original_state.unwrap() != self.expand
    }

    async fn read_label(&self, tx: &Tether) -> Result<Label, ActionError> {
        Label::load_using(self.local_id, tx)
            .await?
            .ok_or_else(|| ModelError::LabelNotFound(self.local_id))
            .map_err(Into::into)
    }
}

impl Action for Expand {
    const TYPE: Type = Type("expand_label");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type Output = ();
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = Expand;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let mut label = action.read_label(tx).await?;

        action.original_state = Some(label.expanded);

        if !action.is_applicable() {
            debug!(
                "No need to apply expand action for label: {:?}",
                action.local_id
            );

            return Ok(());
        }

        action.remote_id.clone_from(&label.remote_id);

        label.expanded = action.expand;

        label.save_using(tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if !action.is_applicable() {
            return Ok(());
        }

        // This will never panic due to the check above
        action.expand = action.original_state.unwrap();

        self.apply_local(action, tx).await
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if !action.is_applicable() {
            return Ok(());
        }

        let remote_id = action.remote_id.clone().ok_or_else(|| {
            ActionError::Other(anyhow!(
                "RemoteID is missing - `apply_local` step should set it up!"
            ))
        })?;

        let responses = Label::patch_expanded(remote_id, action.expand, session.api()).await?;

        action.remote_failed = responses.into_iter().any(|r| r.response.code != 1000);

        Ok(())
    }

    async fn apply_local_post_remote(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        if action.remote_failed {
            error!("Expand remote operation failed for: {:?}", action.remote_id);

            self.revert_local(action, tx).await.map_err(|e| {
                error!("Failed to rollback expand operation: {e}");
                e
            })?;
        }

        Ok(())
    }
}
