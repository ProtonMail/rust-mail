use crate::CoreContextError;
use crate::actions::dependency_builder::ActionDependencyKeysBuilder;
use crate::datatypes::{UnixTimestamp, UserFeatureFlagSource};
use crate::models::UserFeatureFlag;
use proton_action_queue::action::{
    Action, ActionDependencyKey, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler,
    Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::session::Session;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, RunTransaction};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OverrideFlag {
    flag_name: String,
    new_value: bool,
    previous_overridden_value: Option<bool>,
    previous_overridden_at: Option<UnixTimestamp>,
}

impl OverrideFlag {
    #[must_use]
    pub fn new(flag_name: String, new_value: bool) -> Self {
        Self {
            flag_name,
            new_value,
            previous_overridden_value: None,
            previous_overridden_at: None,
        }
    }
}

impl Action for OverrideFlag {
    const TYPE: Type = Type("override_user_feature_flag");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = OverrideFlagHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = CoreContextError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::new()
            .with_optional(ActionDependencyKey::from(format!(
                "feature_flag:{}",
                self.flag_name
            )))
            .build()
    }
}

pub struct OverrideFlagHandler {
    pub api: Session,
}

impl Handler for OverrideFlagHandler {
    type Action = OverrideFlag;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let mut flag = UserFeatureFlag::by_name(&action.flag_name, tx.tether())
            .await?
            .ok_or_else(|| {
                CoreContextError::Other(anyhow::anyhow!(
                    "Feature flag '{}' not found",
                    action.flag_name
                ))
            })?;

        if !flag.writable {
            return Err(CoreContextError::Other(anyhow::anyhow!(
                "Feature flag '{}' is not writable",
                action.flag_name
            )));
        }

        action.previous_overridden_value = flag.overriden_to;
        action.previous_overridden_at = flag.overriden_at;
        flag.overriden_to = Some(action.new_value);
        flag.overriden_at = None;
        flag.save(tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let mut flag = UserFeatureFlag::by_name(&action.flag_name, tx.tether())
            .await?
            .ok_or_else(|| {
                CoreContextError::Other(anyhow::anyhow!(
                    "Feature flag '{}' not found",
                    action.flag_name
                ))
            })?;

        flag.overriden_to = action.previous_overridden_value;
        flag.overriden_at = action.previous_overridden_at;

        flag.save(tx).await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let response = self
            .api
            .put_feature_flag_override(&action.flag_name, action.new_value)
            .await?;

        guard
            .tx(async |tx| {
                let mut flag = UserFeatureFlag::by_name(&action.flag_name, tx)
                    .await?
                    .ok_or_else(|| {
                        CoreContextError::Other(anyhow::anyhow!(
                            "Feature flag '{}' not found",
                            action.flag_name
                        ))
                    })?;

                flag.overriden_at = response
                    .feature
                    .metadata
                    .update_time
                    .map(UnixTimestamp::from);

                if let Some(new_value) = response.feature.variant.into_bool() {
                    flag.enabled = new_value.value;
                    flag.source = UserFeatureFlagSource::Legacy;
                    flag.modify_time = UnixTimestamp::now();
                    flag.writable = response.feature.metadata.writable;
                }

                flag.save(tx).await?;

                Ok::<_, CoreContextError>(())
            })
            .await?;

        Ok(())
    }

    async fn rebase_local(
        &self,
        _this_id: ActionId,
        _action: &mut Self::Action,
        _: &RebaseChangeSet,
        _tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // We do not track feature flag updates as a part of rebasing
        // Nothing to do.
        Ok(())
    }
}
