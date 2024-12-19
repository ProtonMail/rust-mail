#![allow(clippy::module_name_repetitions)]

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

use std::sync::Arc;

use bytes::Bytes;
use muon::client::middleware::{DisplayLogger, Tagger};
use muon::common::IntoDyn;
use muon::dns::{GoogleDoh, Quad9Doh};
use muon::error::ParseAppVersionErr;
use muon::App;
use proton_crypto_account::keys::APIPublicAddressKeys;
use responses::{GetAddressResponse, PutDeleteContactsResponse};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::service::ApiServiceResult;
use crate::services::proton::prelude::*;
use crate::services::proton::proton_impl::{
    MuonStoreImpl, SetCryptoClockLayer, SetDefaultServiceTypeLayer, SetDefaultTimeoutLayer,
};
use crate::session::Config;
use crate::store::Store;

/// Re-export muon for downstream convenience.
pub extern crate muon;

pub mod common;
pub mod prelude;
pub mod request_data;
pub mod requests;
pub mod response_data;
pub mod responses;

mod proton_impl;

/// The Proton Core API base path (v4).
pub const CORE_V4: &str = "/core/v4";

/// The Proton type is just an alias for the muon client.
pub type Proton = muon::Client;

/// An error that can occur when building a Proton client.
#[derive(Debug, Error)]
pub enum BuildError {
    /// The app version could not be parsed.
    #[error(transparent)]
    ParseAppVersion(#[from] ParseAppVersionErr),

    /// The client could not be built.
    #[error(transparent)]
    Build(#[from] muon::Error),
}

/// Builds a new Proton client.
pub fn build<S: Store>(config: Config, store: Arc<RwLock<S>>) -> Result<Proton, BuildError> {
    let app = if let Some(agent) = &config.user_agent {
        App::new(config.app_version)?.with_user_agent(agent)
    } else {
        App::new(config.app_version)?
    };

    let client = Proton::builder(app, MuonStoreImpl::new(config.env_id, store))
        .doh([Quad9Doh.into_dyn(), GoogleDoh.into_dyn()])
        .layer_front(Tagger::default())
        .layer_back(SetCryptoClockLayer)
        .layer_back(SetDefaultServiceTypeLayer)
        .layer_back(SetDefaultTimeoutLayer)
        .layer_back(DisplayLogger::debug())
        .build()?;

    Ok(client)
}

#[allow(async_fn_in_trait)]
pub trait ProtonCore {
    /// GETs a list of addresses.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_addresses(&self) -> ApiServiceResult<GetAddressesResponse>;

    /// GET a single address
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_address_by_id(&self, id: RemoteId) -> ApiServiceResult<GetAddressResponse>;

    /// GETs Captcha details.
    ///
    /// # Parameters
    ///
    /// * `token`     - The Captcha token to use.
    /// * `force_web` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_captcha(&self, options: GetCaptchaOptions) -> ApiServiceResult<String>;

    /// GETs a single contact.
    ///
    /// This returns the full contact record.
    ///
    /// # Parameters
    ///
    /// * `contact_id` - The ID of the contact to get.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_contact(&self, contact_id: RemoteId) -> ApiServiceResult<GetContactResponse>;

    /// GETs a list of contacts.
    ///
    /// This returns basic information — not the full contact record.
    ///
    /// # Parameters
    ///
    /// * `options` - The options to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_contacts(
        &self,
        options: GetContactsOptions,
    ) -> ApiServiceResult<GetContactsResponse>;

    /// GETs a list of emails for contacts.
    ///
    /// This returns basic information — not the full contact record.
    ///
    /// # Parameters
    ///
    /// * `options` - The options to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_contacts_emails(
        &self,
        options: GetContactsEmailsOptions,
    ) -> ApiServiceResult<GetContactsEmailsResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `event_id`            - The ID of the event to get.
    /// * `conversation_counts` - TODO: Document this parameter.
    /// * `message_counts`      - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_event<T>(
        &self,
        event_id: RemoteId,
        options: GetEventOptions,
    ) -> ApiServiceResult<T>
    where
        T: GetEventResponse + for<'de> Deserialize<'de>;

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_events_latest(&self) -> ApiServiceResult<GetEventsLatestResponse>;

    /// Get logo corresponding to an address or a domain.
    ///
    /// # Errors
    ///   * if the request failed.
    async fn get_images_logo(&self, options: GetImagesLogoOptions) -> ApiServiceResult<Bytes>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `email`         - The email address to get keys for.
    /// * `internal_only` - Whether to only get internal keys.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_keys_all(
        &self,
        options: GetKeysAllOptions,
    ) -> ApiServiceResult<APIPublicAddressKeys>;

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_keys_salts(&self) -> ApiServiceResult<GetKeysSaltsResponse>;

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_settings(&self) -> ApiServiceResult<GetSettingsResponse>;

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_tests_ping(&self) -> ApiServiceResult<()>;

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_users(&self) -> ApiServiceResult<GetUsersResponse>;

    /// Method requests to delete contacts which remotes ids were provided.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_delete_contacts(
        &self,
        ids: Vec<RemoteId>,
    ) -> ApiServiceResult<PutDeleteContactsResponse>;
}
