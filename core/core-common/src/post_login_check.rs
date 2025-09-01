use std::sync::Arc;

use crate::datatypes::UserType;
use crate::models::User as UserTable;
use crate::{Context, CoreAccountState};
use async_trait::async_trait;
use proton_core_api::services::proton::{DelinquentState, User, UserId};
use proton_core_api::{metric, services::observability::ObservabilityMetric};
use serde::{Deserialize, Serialize};
use stash::orm::Model as _;
use stash::stash::{Stash, StashConfiguration, StashError};
use thiserror::Error;
use tracing::{error, trace, warn};

/// This enum defines possible error conditions encountered after a successful login,
/// focusing on constraints and limits that might prevent further actions.
#[derive(Debug, Error)]
pub enum PostLoginValidationError {
    /// Indicates that the maximum number of free accounts has been exceeded. Contains the max number of free accounts allowed.
    #[error("The maximum number of free accounts has been exceeded.")]
    FreeAccountLimitExceeded(u64),

    #[error("The account is currently on hold due to an overdue invoice.")]
    DelinquentUser,

    #[error("Error during post login check: {0}")]
    Other(#[from] anyhow::Error),
}

#[async_trait]
pub trait PostLoginValidator: Send + Sync {
    async fn validate(&self, user: &User) -> Result<(), PostLoginValidationError>;
}

#[derive(Clone)]
pub struct DefaultPostLoginValidator {
    /// The optional maximum number of free accounts allowed after login.
    /// If `None`, there is no restriction on free accounts.
    allowed_free_account_count: Option<u64>,
    ctx: Arc<Context>,
}

impl DefaultPostLoginValidator {
    pub fn new(allowed_free_account_count: Option<u64>, ctx: Arc<Context>) -> Self {
        Self {
            allowed_free_account_count,
            ctx,
        }
    }
}
#[async_trait]
impl PostLoginValidator for DefaultPostLoginValidator {
    async fn validate(&self, user: &User) -> Result<(), PostLoginValidationError> {
        let result = self.do_validate(user).await;
        if result.is_err() {
            self.ctx
                .logout_account(user.id.clone())
                .await
                .map_err(anyhow::Error::new)?;
        }
        result
    }
}

impl DefaultPostLoginValidator {
    async fn do_validate(&self, user: &User) -> Result<(), PostLoginValidationError> {
        if matches!(
            user.delinquent,
            DelinquentState::Delinquent | DelinquentState::NotReceived
        ) {
            trace!(
                "Post login check failed, delinquent state is {:?}",
                user.delinquent
            );
            return Err(PostLoginValidationError::DelinquentUser);
        }

        let has_subscription = user.subscribed > 0;
        if !has_subscription
            && let Some(logged_in_free_account_count) =
                Self::get_logged_in_free_account_count(&self.ctx).await
        {
            trace!("Logged-in free accounts: {logged_in_free_account_count}");
            if let Some(allowed_free_account_count) = self.allowed_free_account_count
                && allowed_free_account_count <= logged_in_free_account_count
            {
                return Err(PostLoginValidationError::FreeAccountLimitExceeded(
                    allowed_free_account_count,
                ));
            }
        }
        Ok(())
    }

    /// Retrieves the count of logged-in free (non-subscribed, non-credentialless) accounts.
    /// Errors are logged but do not halt execution.
    async fn get_logged_in_free_account_count(ctx: &Arc<Context>) -> Option<u64> {
        let mut logged_in_free_account_count = 0;

        let accounts = ctx
            .get_accounts()
            .await
            .inspect_err(|err| {
                error!("Error during 'get_accounts' call: {err:?}");
            })
            .ok()?;
        for account in accounts {
            let state = ctx
                .get_account_state(account.remote_id.clone())
                .await
                .inspect_err(|err| {
                    error!("Error during 'get_account_state' call: {err:?}");
                })
                .ok()?;

            if !matches!(state, Some(CoreAccountState::LoggedIn(_))) {
                continue;
            }
            match Self::load_user(ctx, account.remote_id.clone()).await {
                Ok(Some(user)) => {
                    if user.user_type == UserType::CredentialLess {
                        trace!("user '{:?}' is 'CredentialLess'", &user.name);
                        continue;
                    }
                    let has_subscription = user.subscribed.0 > 0;
                    if has_subscription {
                        trace!("user '{:?}' has subscription", &user.name);
                        continue;
                    }
                }
                Ok(None) => continue,
                Err(err) => {
                    error!(
                        "Failed to load User({}) for Account({}): {:?}",
                        &account.remote_id, &account.name_or_addr, err
                    );
                }
            }
            trace!("{} is a Logged-in free account", &account.name_or_addr);
            logged_in_free_account_count += 1;
        }
        Some(logged_in_free_account_count)
    }

    async fn load_user(
        ctx: &Arc<Context>,
        user_id: UserId,
    ) -> Result<Option<UserTable>, StashError> {
        let user_db_path = ctx.user_db_path(&user_id);
        if !user_db_path.exists() {
            warn!("User DB file does not exist: {user_db_path:?}");
            return Ok(None);
        }
        let user_stash = match Stash::new(StashConfiguration {
            path: Some(&user_db_path),
            ..Default::default()
        }) {
            Ok(user_stash) => user_stash,
            Err(err) => {
                error!("Could not open user db: {err:?}");
                return Ok(None);
            }
        };
        UserTable::load(user_id, &user_stash.connection().await?).await
    }
}

metric! {
    #[name = "core_signup_user_check_total"]
    #[version = 1]
    #[doc = "Records the outcomes of the post login user checks."]
    pub struct UserCheckResult {
        pub status: UserCheckStatus,
    }
}

#[derive(PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserCheckStatus {
    Success,
    Failure,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proton_core_api::services::{
        observability::ObservabilityRecorder,
        proton::prelude::{PostMetricsRequestData, PostMetricsRequestElement},
    };
    use serde_json::{self, json};

    fn assert_serialization_deserialization(status: UserCheckStatus, expected_status: &str) {
        let metric = ObservabilityRecorder::into_metrics_element(
            UserCheckResult { status },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();

        let expected_json = format!(
            r#"{{"Name":"core_signup_user_check_total","Version":1,"Timestamp":1741021308,"Data":{{"Labels":{{"status":"{expected_status}"}},"Value":1}}}}"#
        );

        assert_eq!(serialized, expected_json);

        assert_eq!(
            PostMetricsRequestElement {
                name: "core_signup_user_check_total".into(),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({"status": expected_status}),
                    value: 1,
                }
            },
            serde_json::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_user_check_serialization_deserialization_for_all_variants() {
        let statuses = vec![
            (UserCheckStatus::Success, "success"),
            (UserCheckStatus::Failure, "failure"),
        ];

        for (status, expected_status) in statuses {
            assert_serialization_deserialization(status, expected_status);
        }
    }
}
