use crate::CoreContextError;
use crate::actions::dependency_builder::ActionDependencyKeysBuilder;
use crate::datatypes::{FlagMutability, UnixTimestamp, UserFeatureFlagSource};
use crate::models::{ModelExtension, UserFeatureFlag};
use proton_action_queue::action::{
    Action, ActionDependencyKey, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler,
    Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::session::Session;
use serde::{Deserialize, Serialize};
use stash::UserDb;
use stash::orm::Model;
use stash::stash::{Bond, RunTransaction};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OverrideFlag {
    flag_name: String,
    new_value: bool,
    previous_state: Option<PreviousFlagState>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct PreviousFlagState {
    overridden_to: Option<bool>,
    overridden_at: Option<UnixTimestamp>,
}

impl OverrideFlag {
    #[must_use]
    pub fn new(flag_name: String, new_value: bool) -> Self {
        Self {
            flag_name,
            new_value,
            previous_state: None,
        }
    }
}

impl Action<UserDb> for OverrideFlag {
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

impl Handler<UserDb> for OverrideFlagHandler {
    type Action = OverrideFlag;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let mut flag = UserFeatureFlag::by_name(&action.flag_name, tx.tether())
            .await?
            .inspect(|flag| {
                action.previous_state = Some(PreviousFlagState {
                    overridden_to: flag.overridden_to,
                    overridden_at: flag.overridden_at,
                });
            })
            .unwrap_or_else(|| {
                // Only legacy flags can be overridden
                // If the flag was not found in our local cache,
                // we still want to allow user to override it.
                // Worst case: Backend will reject the PUT request.
                UserFeatureFlag::legacy(
                    action.flag_name.clone(),
                    action.new_value,
                    FlagMutability::Mutable,
                    UnixTimestamp::now(),
                )
            });

        if !flag.writable {
            return Err(CoreContextError::Other(anyhow::anyhow!(
                "Feature flag '{}' is not writable",
                action.flag_name
            )));
        }

        flag.overridden_to = Some(action.new_value);
        flag.overridden_at = None;
        flag.save(tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let mut flag = UserFeatureFlag::by_name(&action.flag_name, tx.tether())
            .await?
            .ok_or_else(|| {
                CoreContextError::Other(anyhow::anyhow!(
                    "Feature flag '{}' not found",
                    action.flag_name
                ))
            })?;

        match action.previous_state {
            Some(previous) => {
                flag.overridden_at = previous.overridden_at;
                flag.overridden_to = previous.overridden_to;
                flag.save(tx).await?;
            }
            None => {
                flag.delete(tx).await?;
            }
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_, UserDb>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
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

                flag.overridden_at = response
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
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        // We do not track feature flag updates as a part of rebasing
        // Nothing to do.
        Ok(())
    }
}
