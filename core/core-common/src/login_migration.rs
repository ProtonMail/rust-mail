use crate::Context;
use anyhow::Error;
use async_trait::async_trait;
use proton_core_api::services::proton::UserId;
use stash::{
    macros::Model,
    orm::Model,
    stash::{Bond, StashError, Tether},
};
use std::sync::Arc;
use tracing::instrument;

#[async_trait]
pub trait MigrationSnooper: Send + Sync {
    async fn run(
        &self,
        user_id: &str,
        address_signature_enabled: Option<bool>,
        mobile_signature: Option<String>,
        mobile_signature_enabled: Option<bool>,
    ) -> Result<(), Error>;
}

pub struct NoopMigrationSnooper;

#[async_trait]
impl MigrationSnooper for NoopMigrationSnooper {
    async fn run(
        &self,
        _: &str,
        _: Option<bool>,
        _: Option<String>,
        _: Option<bool>,
    ) -> Result<(), Error> {
        Ok(())
    }
}

pub struct MailMigrationSnooper {
    ctx: Arc<Context>,
}

impl MailMigrationSnooper {
    pub fn new(ctx: Arc<Context>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl MigrationSnooper for MailMigrationSnooper {
    async fn run(
        &self,
        user_id: &str,
        address_signature_enabled: Option<bool>,
        mobile_signature: Option<String>,
        mobile_signature_enabled: Option<bool>,
    ) -> Result<(), Error> {
        self.ctx
            .account_stash()
            .connection()
            .tx(async |tx| {
                PostLoginMobileMigrationPayload {
                    user_id: user_id.into(),
                    address_signature_enabled,
                    mobile_signature,
                    mobile_signature_enabled,
                }
                .save(tx)
                .await
            })
            .await?;

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Model)]
#[TableName("post_login_mobile_migration")]
pub struct PostLoginMobileMigrationPayload {
    #[IdField]
    pub user_id: UserId,

    #[DbField]
    pub address_signature_enabled: Option<bool>,

    #[DbField]
    pub mobile_signature: Option<String>,

    #[DbField]
    pub mobile_signature_enabled: Option<bool>,
}

impl PostLoginMobileMigrationPayload {
    #[instrument(skip_all)]
    pub async fn load(id: &UserId, tether: &Tether) -> Result<Option<Self>, StashError> {
        let exists: Option<i32> = tether
            .query_value_opt(
                "SELECT 1 AS value FROM sqlite_master WHERE type='table' AND name='post_login_mobile_migration'",
                Vec::new(),
            )
            .await?;

        if exists.is_none() {
            Ok(None)
        } else {
            <Self as Model>::load(id.clone(), tether).await
        }
    }

    #[instrument(skip_all)]
    pub async fn save(mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            "CREATE TABLE IF NOT EXISTS post_login_mobile_migration (
                user_id STRING PRIMARY KEY,
                address_signature_enabled BOOL,
                mobile_signature TEXT,
                mobile_signature_enabled BOOL
             )",
            Vec::new(),
        )
        .await?;

        <Self as Model>::save(&mut self, bond).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoreContextError, test_utils::test_context::TestContext};

    #[tokio::test]
    async fn test() {
        let ctx = TestContext::new().await;
        let mut tether = ctx.context().account_stash().connection();

        assert_eq!(
            None,
            PostLoginMobileMigrationPayload::load(&"==abcd2".into(), &tether)
                .await
                .unwrap()
        );

        tether
            .tx::<_, _, CoreContextError>(async |tx| {
                for id in 0..5 {
                    PostLoginMobileMigrationPayload {
                        user_id: format!("==abcd{id}").into(),
                        address_signature_enabled: Some(true),
                        mobile_signature: Some("mobile signature".into()),
                        mobile_signature_enabled: Some(false),
                    }
                    .save(tx)
                    .await?;
                }

                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(
            Some(PostLoginMobileMigrationPayload {
                user_id: "==abcd2".into(),
                address_signature_enabled: Some(true),
                mobile_signature: Some("mobile signature".into()),
                mobile_signature_enabled: Some(false),
            }),
            PostLoginMobileMigrationPayload::load(&"==abcd2".into(), &tether)
                .await
                .unwrap()
        );
    }
}
