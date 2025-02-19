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

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use muon::client::middleware::{DisplayLogger, Tagger};
use muon::common::{IntoDyn, RetryPolicy};
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
use crate::status_watcher::StatusWatcher;
use crate::store::Store;

/// Re-export muon for downstream convenience.
pub extern crate muon;

pub mod common;
pub mod prelude;
pub mod request_data;
pub mod requests;
pub mod response_data;
pub mod responses;

pub use self::proton_impl::{
    HALF_MINUTE_TIMEOUT, ONE_MINUTE_TIMEOUT, ONE_SECOND_TIMEOUT, QUARTER_SECOND_TIMEOUT,
};

mod proton_impl;

/// The Proton Core API base path (v4).
pub const CORE_V4: &str = "/core/v4";

/// The Proton Core API base path (v5).
pub const CORE_V5: &str = "/core/v5";

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
pub fn build<S: Store>(
    config: Config,
    store: Arc<RwLock<S>>,
    status_watcher: StatusWatcher,
) -> Result<Proton, BuildError> {
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
        .layer_back(status_watcher)
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
    async fn get_address_by_id(&self, id: AddressId) -> ApiServiceResult<GetAddressResponse>;

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
    async fn get_contact(&self, contact_id: ContactId) -> ApiServiceResult<GetContactResponse>;

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
        event_id: EventId,
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

    /// The ping endpoint for testing connectivity.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    fn get_tests_ping(
        &self,
        timeout: Option<Duration>,
        retry: Option<RetryPolicy>,
    ) -> impl Future<Output = ApiServiceResult<()>> + Send;

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
        ids: Vec<ContactId>,
    ) -> ApiServiceResult<PutDeleteContactsResponse>;

    /// Method requests to delete label
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to delete.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn delete_label(&self, label_id: LabelId) -> ApiServiceResult<()>;

    /// Method requests all labels with given label type
    ///
    /// # Parameters
    ///
    /// * `label_type` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_labels(&self, label_type: LabelType) -> ApiServiceResult<GetLabelsResponse>;

    /// Method to get labels by their IDs.
    /// Makes a POST request to the `/labels/by-ids` endpoint.
    /// Names refer to the fact labels are acquired by their IDs.
    /// HTTP `GET` method is not suppose to have a body,
    /// so POST method is used instead.
    ///
    ///
    /// # Parameters
    ///
    /// * `label_ids` - List of label IDs to get.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_labels_by_ids(
        &self,
        label_ids: Vec<LabelId>,
    ) -> ApiServiceResult<GetLabelsResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `body` - The body to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn post_labels(&self, body: PostLabelsRequest) -> ApiServiceResult<PostLabelsResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to update.
    /// * `body`     - The body to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_label(
        &self,
        label_id: LabelId,
        body: PutLabelRequest,
    ) -> ApiServiceResult<PutLabelResponse>;

    /// This method is used to patch an existing label.
    /// The `label_id` is used to identify the label to patch.
    /// Body contains expanded and notify fields.
    /// Expanded is a boolean that indicates if the label is expanded.
    /// For example if the folder is expanded in the UI.
    /// Notify is a boolean that indicates if the user should be notified
    /// about new messages in the label. By default both of them are disabled.
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to patch.
    /// * `body` - Json body to use in the patch request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn patch_label(
        &self,
        label_id: LabelId,
        body: PatchLabelRequest,
    ) -> ApiServiceResult<PatchLabelResponse>;

    /// This method is used to register device for push notifications.
    /// The registering will delete any duplicate having the same (User ID, Product, Device Token) from different sessions.
    /// If the registering is done from a session already having a registered device, the existing device will be replaced with the new one.
    ///
    /// # Parameters
    ///
    /// * `body` - Json body to use in the post request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn register_device(&self, body: RegisterDeviceRequest) -> ApiServiceResult<()>;
}
