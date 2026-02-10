//! This module provides the [`Atlas`] environment.

use crate::app::{AppVersion, Platform};
use crate::common::{Host, Server};
use crate::env::Env;

/// An atlas API environment.
///
/// This environment is used for internal testing.
/// Clients using this environment will connect to `https://*.proton.black/`.
#[must_use]
#[derive(Debug, Default)]
pub enum Atlas {
    /// The standard Atlas environment, with no scientist name.
    ///
    /// Clients using this will send requests to `https://proton.black/api`.
    #[default]
    Standard,

    /// An Atlas environment with a specific scientist name.
    ///
    /// Clients using this will send requests to `https://foo.proton.black/api`,
    /// where `foo` is the scientist name provided.
    Scientist(String),
}

impl Env for Atlas {
    fn servers(&self, version: &AppVersion) -> Vec<Server> {
        let base = String::from("proton.black");
        let (host, path) = if let Some(name) = version.name() {
            let (plat, prod) = (name.platform(), name.product());
            match (self, plat) {
                // Specific to Web
                (Atlas::Standard, Platform::Web) => (format!("{prod}.{base}"), "/api"),
                (Atlas::Scientist(name), Platform::Web) => {
                    (format!("{prod}.{name}.{base}"), "/api")
                }
                // Other platforms
                (Atlas::Standard, _) => (format!("{prod}-api.{base}"), "/"),
                (Atlas::Scientist(name), _) => (format!("{prod}-api.{name}.{base}"), "/"),
            }
        } else {
            match self {
                Atlas::Standard => (base, "/api"),
                Atlas::Scientist(name) => (format!("{name}.{base}"), "/api"),
            }
        };

        if let Ok(host) = Host::direct(&host) {
            vec![Server::https(host, path)]
        } else {
            panic!("invalid atlas host: {host}");
        }
    }
}

if_sealed! {
    impl crate::Sealed for Atlas {}
}
