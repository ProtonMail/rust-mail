//! The Proton API service.
//!
//! This module provides a service that can be used to make requests to the
//! Proton API. Each method provided should match 1:1 with an API endpoint, and
//! follow the naming convention of the endpoint. For example, the endpoint
//! `GET /contacts` should have a method provided called `get_contacts()`.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the data structures provided by this service should mirror the API
//! endpoint definitions, and NOT have any business logic or other
//! functionality.
//!
//! To be clear, they should only contain data, and not methods; should not be
//! saved in the database; and should not be used for anything except providing
//! an interface for data exchange.
//!
//! The goal is not to provide a semantic representation of actions, but a
//! strict and closely-coupled interface to the API.
//!
//! Everything in this service should be self-contained as much as possible, and
//! should be considered encapsulated and separate from the main application,
//! including the application's data. Types should be converted back and forth
//! as necessary, but generally not used in both places.
//!
//! # Example illustration
//!
//! Let's consider the case of a user. The application may have a `User` struct
//! that is used to represent a user in the application. From time to time it
//! will be necessary to interact with the API to sync data relevant to that
//! `User`. To do this, the necessary information should be used to prepare data
//! to send to the API, such as a `PostUserRequest` struct containing a child
//! data type of `User`. This latter `User` is not the same struct as that used
//! in the application, but rather, a data-only mirroring of the data the API
//! needs to receive.
//!
//! Now let's consider retrieving a user record. The API response might define
//! a `User` structure that is the same as the one accepted via `POST` — if so,
//! this could go into [`common`]. Otherwise, we will need two `User` structs,
//! one in [`request_data`] and one in [`response_data`]. Neither one of these
//! is the same as the one used inside the main application.
//!
//! Once the data is retrieved, the data required by the application can be
//! extracted from the response and converted into the application's `User`
//! struct. This struct would then be the one containing various methods and
//! other functionality, and would get saved to the database.
//!

use crate::services::proton::layers::CookieJarLayer;
use crate::services::proton::layers::SetCryptoClockLayer;
use crate::services::proton::layers::SetDefaultServiceTypeLayer;
use crate::services::proton::layers::SetDefaultTimeoutLayer;
use crate::services::proton::store::MuonStoreImpl;
use crate::session::Config;
use crate::store::Store;
use crate::verification::ChallengeNotifierLayer;
use crate::verification::DynChallengeNotifier;
use cookie::CookieJar;
use muon::App;
use muon::client::InfoProvider;
use muon::client::middleware::{DisplayLogger, Tagger};
use muon::common::ConstProxy;
use muon::common::IntoDyn;
use muon::common::ParseEndpointErr;
use muon::dns::{GoogleDoh, Quad9Doh};
use muon::error::ParseAppVersionErr;
use proton_task_service::Tokio;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

mod auth;
mod core;
mod data;
mod layers;
mod macros;
mod payments;
mod store;

pub type Proton = muon::Client;

pub mod common;
pub mod prelude;
pub mod traits;

pub use self::auth::*;
pub use self::core::*;
pub use self::data::*;
pub use self::payments::*;
pub use muon;
use proton_network_monitor_service::ConnectionMonitor;

/// An error that can occur when building a Proton client.
#[derive(Debug, Error)]
pub enum BuildError {
    /// The app version could not be parsed.
    #[error(transparent)]
    ParseAppVersion(#[from] ParseAppVersionErr),

    /// A proxy endpoint could not be parsed.
    #[error(transparent)]
    ParseEndpoint(#[from] ParseEndpointErr),

    /// The client could not be built.
    #[error(transparent)]
    Build(#[from] muon::Error),
}

/// Builds a new Proton client.
pub async fn build<S: Store>(
    config: &Arc<Config>,
    store: &Arc<RwLock<S>>,
    connection_monitor: &ConnectionMonitor,
    notifier: DynChallengeNotifier,
    info_provider: Option<Arc<dyn InfoProvider>>,
    allow_doh: bool,
) -> Result<Proton, BuildError> {
    let store = MuonStoreImpl::new(&config.env_id, store);

    let app = if let Some(agent) = &config.user_agent {
        App::new(&config.app_version)?.with_user_agent(agent)
    } else {
        App::new(&config.app_version)?
    };

    let mut builder = (Proton::builder_async(app, store).await)
        .layer_front(Tagger::default())
        .layer_front(connection_monitor.clone())
        .layer_back(SetCryptoClockLayer)
        .layer_back(SetDefaultServiceTypeLayer)
        .layer_back(SetDefaultTimeoutLayer)
        .layer_back(ChallengeNotifierLayer::new(notifier))
        .layer_back(CookieJarLayer::new(CookieJar::new()))
        .layer_back(DisplayLogger::debug())
        .spawner(Tokio::spawner());

    if let Some(resolver) = config.resolver.clone() {
        builder = builder.resolver(resolver);
    }

    if let Some(proxy) = &config.proxy {
        builder = builder.proxy(ConstProxy::new(proxy.parse()?));
    }

    if allow_doh {
        builder = builder.doh([Quad9Doh.into_dyn(), GoogleDoh.into_dyn()]);
    }

    let mut client = builder.build()?;

    if let Some(info_provider) = info_provider {
        client = client.with_info_provider(info_provider);
    }

    Ok(client)
}
