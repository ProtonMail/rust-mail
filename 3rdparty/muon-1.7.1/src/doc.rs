#![allow(warnings)]

//! This module contains code snippets for documentation

use crate::env::EnvId;
use crate::flow::LoginFlowData;
use crate::store::{Store, StoreError};
use crate::{Auth, Client, Error};
use async_trait::async_trait;

/// todo
#[derive(Debug)]
pub struct MyPersistenceStorage;

impl MyPersistenceStorage {
    /// todo
    pub fn prod() -> Self {
        Self
    }
}

#[async_trait]
impl Store for MyPersistenceStorage {
    fn env(&self) -> EnvId {
        EnvId::new_atlas()
    }

    async fn get_auth(&self) -> Auth {
        Auth::None
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<Auth, StoreError> {
        Ok(auth)
    }
}

/// todo
pub fn show_user_cant_login_modal(_: impl Into<Error>) {}

/// todo
pub fn display_authenticated_user_info(_: &Client) {}

/// todo
pub fn load_user_preferences(_: &str) -> Result<(), Error> {
    Ok(())
}

/// todo
pub fn ask_user_for_2fa() -> String {
    "".to_owned()
}

/// todo
pub fn ask_user_for_mbp() -> String {
    "".to_owned()
}

/// todo
pub fn unlock_pgp_key(_: &Client, _: &str) {}
