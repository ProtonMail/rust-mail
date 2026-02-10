use anyhow::Error;
use async_trait::async_trait;

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
