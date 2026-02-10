use std::any::Any;

#[async_trait::async_trait]
pub trait Service: Any + Send + Sync + 'static {
    type Error: Send + Sync + 'static;

    async fn init(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}
