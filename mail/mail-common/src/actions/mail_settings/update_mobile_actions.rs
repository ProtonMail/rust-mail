use crate::AppError;
use crate::actions::MailActionError;
use crate::datatypes::{MobileAction, MobileSetting, MobileSettings};
use crate::models::MailSettings;
use anyhow::Context;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_core_api::session::Session;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, RunTransaction};
use tracing::warn;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ToolbarType {
    List,
    Message,
    Conversation,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateMobileActions {
    pub toolbar_type: ToolbarType,
    pub actions: Vec<MobileAction>,
    pub new_mobile_settings: Option<MobileSettings>,
    pub old_mobile_settings: Option<MobileSettings>,
    pub is_default: bool,
}

impl UpdateMobileActions {
    pub fn new(
        toolbar_type: ToolbarType,
        actions: Vec<MobileAction>,
        is_default: bool,
    ) -> Result<Self, AppError> {
        if actions.len() > 5 {
            return Err(AppError::Other(anyhow::anyhow!(
                "Maximum 5 toolbar actions allowed, got {}",
                actions.len()
            )));
        }

        Self::validate_actions_for_context(&actions, &toolbar_type)?;

        Ok(Self {
            toolbar_type,
            actions,
            new_mobile_settings: None,
            old_mobile_settings: None,
            is_default,
        })
    }

    fn validate_actions_for_context(
        actions: &[MobileAction],
        toolbar_type: &ToolbarType,
    ) -> Result<(), AppError> {
        if actions.is_empty() {
            return Ok(());
        }

        let all_valid = match toolbar_type {
            ToolbarType::List => MobileAction::all_list_actions(),
            ToolbarType::Message => MobileAction::all_message_actions(),
            ToolbarType::Conversation => MobileAction::all_conversation_actions(),
        };

        for action in actions {
            if !all_valid.contains(action) {
                return Err(AppError::Other(anyhow::anyhow!(
                    "Action {:?} is not valid for {:?} toolbar",
                    action,
                    toolbar_type
                )));
            }
        }

        Ok(())
    }
}

impl Action for UpdateMobileActions {
    const TYPE: Type = Type("update_mobile_actions");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UpdateMobileActionsHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct UpdateMobileActionsHandler {
    pub api: Session,
}

impl Handler for UpdateMobileActionsHandler {
    type Action = UpdateMobileActions;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let mut mail_settings = MailSettings::get_or_default(bond.tether()).await;
        let mut mobile_settings = mail_settings.mobile_settings.unwrap_or_default();

        action.old_mobile_settings = Some(mobile_settings.clone());

        let mobile_setting = MobileSetting {
            actions: action.actions.clone(),
            is_custom: !action.is_default,
        };

        match action.toolbar_type {
            ToolbarType::List => {
                mobile_settings.list_toolbar = mobile_setting;
            }
            ToolbarType::Message => {
                mobile_settings.message_toolbar = mobile_setting;
            }
            ToolbarType::Conversation => {
                mobile_settings.conversation_toolbar = mobile_setting;
            }
        }

        mail_settings.mobile_settings = Some(mobile_settings.clone());
        action.new_mobile_settings = Some(mobile_settings);

        mail_settings.save(bond).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if let Some(old_mobile_settings) = action.old_mobile_settings.clone() {
            let mut mail_settings = MailSettings::get_or_default(bond.tether()).await;
            mail_settings.mobile_settings = Some(old_mobile_settings);
            mail_settings.save(bond).await?;
        } else {
            warn!("No old mobile settings found, cannot revert");
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        if let Some(mobile_settings) = action.new_mobile_settings.clone() {
            MailSettings::update_mobile_settings(&self.api, mobile_settings)
                .await
                .context("Failed to sync mobile settings to API")?;
        } else {
            warn!("No new mobile settings found, cannot apply remotely");
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/actions/mail_settings/update_mobile_actions.rs"]
mod tests;
