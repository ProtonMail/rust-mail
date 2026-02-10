//! ## Env
//!
//! This module defines types representing an API environment. An environment
//! defines the servers the client should use, for a given platform and product.
//! The client uses the environment to determine the API endpoints to connect
//! to and how to verify their TLS certificates.
//!
//! All types related to the Proton environment.
//!
//! ### Examples (atlas)
//! ```
//! # tokio_test::block_on(async {
//! use muon::env::EnvId;
//! use muon::{App, Client, GET};
//! let app = App::new("windows-vpn@1.0.0")?;
//! let atlas = EnvId::new_atlas();
//! let client = Client::new(app, atlas)?;
//! let res = client.send(GET!("/tests/ping")).await?;
//! # anyhow::Ok(())
//! # });
//! ```
//!
//! ### NB
//! By default, environments can not be extended unless the `unsealed` feature
//! is activated.

use crate::app::AppVersion;
use crate::common::{Host, IntoDyn, Server};
use crate::tls::TlsPinSet;
use crate::util::IntoIterExt;
use crate::Sealed;
use derive_more::{AsRef, FromStr};
use muon_proc::derive_dyn;
use std::sync::Arc;
use thiserror::Error;

mod atlas;
pub use atlas::Atlas;

mod prod;
pub use prod::Prod;

/// An API environment.
///
/// This provides information about the environment in which the client
/// operates, such as the API endpoints and their TLS pins.
///
/// This is a sealed trait; it cannot be implemented outside of this crate.
/// However, it can be implemented by enabling the `unsealed` feature.
#[derive_dyn(Debug)]
pub trait Env: Sealed + Send + Sync + 'static {
    /// Get the servers available for the given app version.
    fn servers(&self, version: &AppVersion) -> Vec<Server>;

    /// Get the TLS pins for a given host, if any.
    fn pins(&self, _host: &Host) -> Option<&TlsPinSet> {
        None
    }
}

/// A dynamic environment.
pub type DynEnv = Arc<dyn Env>;

impl Env for DynEnv {
    fn servers(&self, version: &AppVersion) -> Vec<Server> {
        self.as_ref().servers(version)
    }

    fn pins(&self, host: &Host) -> Option<&TlsPinSet> {
        self.as_ref().pins(host)
    }
}

impl<This: Env> IntoDyn<DynEnv> for This {
    fn into_dyn(self) -> DynEnv {
        Arc::new(self)
    }
}

impl IntoDyn<DynEnv> for &DynEnv {
    fn into_dyn(self) -> DynEnv {
        self.to_owned()
    }
}

if_sealed! {
    impl crate::Sealed for DynEnv {}
}

/// An environment identifier.
#[must_use]
#[derive(Debug, Clone)]
pub enum EnvId {
    /// The production environment.
    ///
    /// This refers to the [`Prod`] environment.
    Prod,

    /// The Atlas environment.
    ///
    /// This refers to the [`Atlas`] environment.
    Atlas(Option<String>),

    /// A custom environment.
    ///
    /// This provides a user-defined [`Env`] implementation.
    /// Such environments are not supported by default; the [`Env`] trait is
    /// sealed and cannot be implemented outside of this crate. However, this
    /// restriction can be lifted by enabling the `unsealed` feature.
    Custom(DynEnv),
}

impl EnvId {
    /// Create a new prod environment identifier.
    pub fn new_prod() -> Self {
        Self::Prod
    }

    /// Create a new atlas environment identifier.
    pub fn new_atlas() -> Self {
        Self::Atlas(None)
    }

    /// Create a new atlas environment identifier with the given scientist name.
    pub fn new_atlas_name(name: impl AsRef<str>) -> Self {
        Self::Atlas(Some(name.as_ref().to_owned()))
    }

    /// Create a new custom environment identifier.
    pub fn new_custom(env: impl Env) -> Self {
        Self::Custom(env.into_dyn())
    }

    /// Build an environment from this identifier.
    #[must_use]
    pub fn build(self) -> DynEnv {
        match self {
            Self::Prod => Prod::default().into_dyn(),
            Self::Atlas(None) => Atlas::Standard.into_dyn(),
            Self::Atlas(Some(name)) => Atlas::Scientist(name).into_dyn(),
            Self::Custom(env) => env,
        }
    }
}

/// An error that can occur when parsing an environment identifier.
#[derive(Debug, Error)]
#[error("invalid environment identifier: {0}")]
pub struct ParseEnvIdErr(String);

impl FromStr for EnvId {
    type Err = ParseEnvIdErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split(':').into_vec().as_slice() {
            ["prod"] => Ok(Self::new_prod()),
            ["atlas"] => Ok(Self::new_atlas()),
            ["atlas", name] => Ok(Self::new_atlas_name(name)),

            _ => Err(ParseEnvIdErr(s.to_owned()))?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_id() {
        let env: EnvId = "prod".parse().unwrap();
        assert!(matches!(env, EnvId::Prod));

        let env: EnvId = "atlas".parse().unwrap();
        assert!(matches!(env, EnvId::Atlas(None)));

        let env: EnvId = "atlas:scientist".parse().unwrap();
        assert!(matches!(env, EnvId::Atlas(Some(name)) if name == "scientist"));
    }
}
