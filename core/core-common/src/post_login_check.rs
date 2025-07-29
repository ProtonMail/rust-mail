use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use proton_core_api::services::proton::{DelinquentState, User, UserType};
use thiserror::Error;
use tracing::error;

use crate::Context;

/// This enum defines possible error conditions encountered after a successful login,
/// focusing on constraints and limits that might prevent further actions.
#[derive(Debug, Error)]
pub enum PostLoginValidationError {
    /// Indicates that the maximum number of free accounts has been exceeded.
    #[error("The maximum number of free accounts has been exceeded.")]
    FreeAccountLimitExceeded,

    #[error("The account is currently on hold due to an overdue invoice.")]
    DelinquentUser,

    #[error("Error during post login check: {0}")]
    Other(anyhow::Error),
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
        if matches!(
            user.delinquent,
            DelinquentState::Delinquent | DelinquentState::NotReceived
        ) {
            return Err(PostLoginValidationError::DelinquentUser);
        }

        if user.user_type == UserType::CredentialLess {
            return Ok(());
        }

        let has_subscription = user.subscribed > 0;
        if has_subscription {
            return Ok(());
        }

        let account_count = self
            .ctx
            .get_accounts()
            .await
            .map_err(|err| {
                error!("Error during 'get_accounts' call: {err:?}");
                PostLoginValidationError::Other(
                    anyhow!(err).context("Error during 'get_accounts' call: {err:?}"),
                )
            })?
            .into_iter()
            .filter(|account| account.is_ready)
            .count() as u64;
        if let Some(allowed_free_account_count) = self.allowed_free_account_count
            && allowed_free_account_count < account_count
        {
            return Err(PostLoginValidationError::FreeAccountLimitExceeded);
        }
        Ok(())
    }
}
