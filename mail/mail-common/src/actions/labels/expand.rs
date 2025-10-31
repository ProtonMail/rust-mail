use crate::datatypes::RollbackItemType;
use crate::models::RollbackItem;
use crate::{AppError, actions::MailActionError};
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::Session;
use proton_core_common::actions::dependency_builder::{
    ActionDependencyKeysBuilder, LocalIdActionDepExt,
};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::Label;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;
use tracing::{debug, info};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Expand {
    local_id: LocalLabelId,
    remote_id: Option<LabelId>,
    expand: bool,
    original_state: Option<bool>,
}

impl Expand {
    #[allow(clippy::self_named_constructors)]
    pub fn expand(local_id: LocalLabelId) -> Self {
        Self::new(local_id, true)
    }

    pub fn collapse(local_id: LocalLabelId) -> Self {
        Self::new(local_id, false)
    }

    fn new(local_id: LocalLabelId, expand: bool) -> Self {
        Self {
            local_id,
            expand,
            remote_id: None,
            original_state: None,
        }
    }
}

impl Action for Expand {
    const TYPE: Type = Type("expand_label");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ExpandHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::new()
            .with_required_related(self.local_id)
            .with_required(self.local_id.to_custom_dependency_key("mail-label-expand"))
            .build()
    }
}

pub struct ExpandHandler {
    pub api: Session,
}

impl Handler for ExpandHandler {
    type Action = Expand;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let mut label = Label::load(action.local_id, tx)
            .await?
            .ok_or_else(|| AppError::LabelNotFound(action.local_id))?;

        action.original_state = action.original_state.or(Some(label.expanded));

        let label_is_equal_action = action
            .original_state
            .filter(|original_state| *original_state == action.expand)
            .filter(|_| label.expanded == action.expand)
            .is_some();

        if label_is_equal_action {
            debug!(
                "No need to apply expand action for label: {:?}",
                action.local_id
            );

            return Ok(());
        }

        action.remote_id.clone_from(&label.remote_id);

        label.expanded = action.expand;
        info!(
            "Patching expanded for {:?} value={}",
            label.id(),
            action.expand
        );

        label.save(tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let Some(original_state) = action
            .original_state
            .filter(|original_state| *original_state != action.expand)
        else {
            return Ok(());
        };

        action.expand = original_state;

        self.apply_local(id, action, tx).await?;

        if let Some(remote_id) = action.remote_id.clone() {
            RollbackItem::new(remote_id.to_string(), RollbackItemType::Label)
                .save(tx)
                .await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let action_equal_original_state = action
            .original_state
            .filter(|original_state| *original_state == action.expand)
            .is_some();

        if action_equal_original_state {
            return Ok(());
        }

        let remote_id = match action.remote_id.clone() {
            Some(remote_id) => remote_id,
            None => {
                let label = Label::load(action.local_id, guard.tether())
                    .await?
                    .ok_or_else(|| AppError::LabelNotFound(action.local_id))?;

                action.original_state = Some(label.expanded);

                let label_is_equal_action = action
                    .original_state
                    .filter(|_| label.expanded == action.expand)
                    .is_some();

                if label_is_equal_action {
                    return Ok(());
                }

                label
                    .remote_id
                    .clone()
                    .ok_or_else(|| AppError::LabelDoesNotHaveRemoteId(action.local_id))?
            }
        };

        info!(
            "Patching expanded for {:?} value={}",
            remote_id, action.expand
        );

        Label::patch_expanded(remote_id, action.expand, &self.api).await?;

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
