use crate::auth::Auth;
use crate::env::EnvId;
use crate::store::{Store, StoreError};
use async_trait::async_trait;

#[derive(Debug)]
pub struct InMemoryStore {
    auth: Auth,
    env: EnvId,
}

impl InMemoryStore {
    pub fn new(env: EnvId, auth: Option<Auth>) -> Self {
        let auth = auth.unwrap_or_default();

        Self { env, auth }
    }
}

#[async_trait]
impl Store for InMemoryStore {
    fn env(&self) -> EnvId {
        self.env.clone()
    }

    async fn get_auth(&self) -> Auth {
        self.auth.clone()
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<Auth, StoreError> {
        self.auth = auth;

        Ok(self.auth.clone())
    }
}
