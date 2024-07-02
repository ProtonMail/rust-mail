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

pub mod common;
pub mod request_data;
pub mod requests;
pub mod response_data;
pub mod responses;

use crate::service::{ApiService, ApiServiceError, Request, NO_PARAMS};
use crate::services::proton::common::RemoteId;
use crate::services::proton::requests::{
    GetContactsEmailsOptions, GetContactsOptions, GetEventOptions,
};
use crate::services::proton::responses::{
    GetAddressesResponse, GetContactResponse, GetContactsEmailsResponse, GetContactsResponse,
    GetEventResponse, GetEventsLatestResponse, GetSettingsResponse, GetUsersResponse,
};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, Url,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// A service for communicating with the Proton API.
#[derive(Clone, Debug)]
pub struct Proton {
    /// The Reqwest HTTP client which is used internally.
    client: Client,

    /// The base URL for the external service.
    base_url: Url,

    /// A collection of headers to send with every request.
    headers: HeaderMap,
}

impl ApiService for Proton {
    fn base_url(&self) -> &Url {
        &self.base_url
    }

    fn client(&self) -> &Client {
        &self.client
    }

    fn headers(&self) -> HeaderMap {
        self.headers.clone()
    }

    async fn on_error<J, T>(
        &self,
        error: ApiServiceError,
        _request: Request<J>,
    ) -> Result<T, ApiServiceError>
    where
        J: Clone + Serialize + Send + Sync,
        T: DeserializeOwned,
    {
        Err(error)
    }

    fn set_header(&mut self, name: &str, value: &str) {
        self.headers.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_bytes(value.as_bytes()).unwrap(),
        );
    }
}

impl Proton {
    const BASE_PATH: &'static str = "/core/v4";

    /// Generates a new external API service handler.
    ///
    /// # Parameters
    ///
    /// * `base_url` - The API base URL.
    /// * `headers`  - The headers to send with every request.
    ///
    pub fn new(base_url: Url, headers: Option<HeaderMap>) -> Self {
        Self {
            client: Client::new(),
            base_url,
            headers: headers.unwrap_or_default(),
        }
    }

    /// GETs a list of addresses.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_addresses(&self) -> Result<GetAddressesResponse, ApiServiceError> {
        self.get(&format!("{}/addresses", Self::BASE_PATH), NO_PARAMS, None)
            .await
    }

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
    pub async fn get_contact(
        &self,
        contact_id: RemoteId,
    ) -> Result<GetContactResponse, ApiServiceError> {
        self.get(&format!("api/contacts/{contact_id}"), NO_PARAMS, None)
            .await
    }

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
    pub async fn get_contacts(
        &self,
        options: GetContactsOptions,
    ) -> Result<GetContactsResponse, ApiServiceError> {
        self.get("api/contacts", Some(options), None).await
    }

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
    pub async fn get_contacts_emails(
        &self,
        options: GetContactsEmailsOptions,
    ) -> Result<GetContactsEmailsResponse, ApiServiceError> {
        self.get("api/contacts/emails", Some(options), None).await
    }

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
    pub async fn get_event<T>(
        &self,
        event_id: RemoteId,
        conversation_counts: bool,
        message_counts: bool,
    ) -> Result<T, ApiServiceError>
    where
        T: GetEventResponse + for<'de> Deserialize<'de>,
    {
        self.get(
            &format!("core/v5/events/{event_id}"),
            Some(GetEventOptions {
                conversation_counts,
                message_counts,
            }),
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_events_latest(&self) -> Result<GetEventsLatestResponse, ApiServiceError> {
        self.get(
            &format!("{}/events/latest", Self::BASE_PATH),
            NO_PARAMS,
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_settings(&self) -> Result<GetSettingsResponse, ApiServiceError> {
        self.get(&format!("{}/settings", Self::BASE_PATH), NO_PARAMS, None)
            .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_tests_ping(&self) -> Result<(), ApiServiceError> {
        self.get("tests/ping", NO_PARAMS, None).await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_users(&self) -> Result<GetUsersResponse, ApiServiceError> {
        self.get(&format!("{}/users", Self::BASE_PATH), NO_PARAMS, None)
            .await
    }
}
