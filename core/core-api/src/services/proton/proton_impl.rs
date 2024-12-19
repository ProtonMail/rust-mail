use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::DateTime;
use futures::TryFutureExt;
use muon::common::Timeout;
use muon::common::{BoxFut, Sender, SenderLayer, ServiceType};
use muon::env::EnvId;
use muon::error::ErrorKind as MuonErrorKind;
use muon::store::{Store as MuonStore, StoreError as MuonStoreError};
use muon::util::ProtonRequestExt;
use muon::Result as MuonResult;
use muon::{serde_to_query, Status};
use muon::{ProtonRequest, ProtonResponse};
use muon::{GET, PUT};
use proton_crypto_account::keys::APIPublicAddressKeys;
use proton_crypto_account::proton_crypto::crypto::UnixTimestamp;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::auth::Auth;
use crate::crypto_clock::server_crypto_clock;
use crate::service::{ApiServiceError, ApiServiceResult};
use crate::services::proton::prelude::*;
use crate::services::proton::CORE_V4;
use crate::services::proton::{Proton, ProtonCore};
use crate::store::Store;

impl ProtonCore for Proton {
    async fn get_addresses(&self) -> ApiServiceResult<GetAddressesResponse> {
        Ok(GET!("{CORE_V4}/addresses")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_address_by_id(&self, id: RemoteId) -> ApiServiceResult<GetAddressResponse> {
        Ok(GET!("{CORE_V4}/addresses/{id}")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_captcha(&self, options: GetCaptchaOptions) -> ApiServiceResult<String> {
        Ok(GET!("{CORE_V4}/captcha")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_string()?)
    }

    async fn get_contact(&self, contact_id: RemoteId) -> ApiServiceResult<GetContactResponse> {
        Ok(GET!("/contacts/{contact_id}")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_contacts(
        &self,
        options: GetContactsOptions,
    ) -> ApiServiceResult<GetContactsResponse> {
        Ok(GET!("/contacts")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_contacts_emails(
        &self,
        options: GetContactsEmailsOptions,
    ) -> ApiServiceResult<GetContactsEmailsResponse> {
        Ok(GET!("/contacts/emails")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_delete_contacts(
        &self,
        ids: Vec<RemoteId>,
    ) -> ApiServiceResult<PutDeleteContactsResponse> {
        Ok(PUT!("/contacts/delete")
            .body_json(PutDeleteContacts { ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_event<T>(
        &self,
        event_id: RemoteId,
        options: GetEventOptions,
    ) -> ApiServiceResult<T>
    where
        T: GetEventResponse + for<'de> Deserialize<'de>,
    {
        Ok(GET!("{CORE_V4}/events/{event_id}")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_events_latest(&self) -> ApiServiceResult<GetEventsLatestResponse> {
        Ok(GET!("{CORE_V4}/events/latest")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_images_logo(&self, options: GetImagesLogoOptions) -> ApiServiceResult<Bytes> {
        Ok(GET!("{CORE_V4}/images/logo")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body()
            .into())
    }

    async fn get_keys_all(
        &self,
        options: GetKeysAllOptions,
    ) -> ApiServiceResult<APIPublicAddressKeys> {
        Ok(GET!("{CORE_V4}/keys/all")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_keys_salts(&self) -> ApiServiceResult<GetKeysSaltsResponse> {
        Ok(GET!("{CORE_V4}/keys/salts")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_settings(&self) -> ApiServiceResult<GetSettingsResponse> {
        Ok(GET!("{CORE_V4}/settings")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_tests_ping(&self) -> ApiServiceResult<()> {
        GET!("{CORE_V4}/tests/ping").send_with(self).await?.ok()?;

        Ok(())
    }

    async fn get_users(&self) -> ApiServiceResult<GetUsersResponse> {
        Ok(GET!("{CORE_V4}/users")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }
}

#[allow(clippy::redundant_closure_for_method_calls)]
impl From<muon::Error> for ApiServiceError {
    fn from(e: muon::Error) -> Self {
        // Check if the error is the result of a timeout.
        if e.source().is_some_and(|s| s.is::<Timeout>()) {
            return Self::Timeout(e.to_string());
        }

        // Otherwise, match on the kind of error we received.
        match e.kind() {
            MuonErrorKind::Tls
            | MuonErrorKind::Resolve
            | MuonErrorKind::Dial
            | MuonErrorKind::Connect => Self::ConnectionError(e.to_string()),

            MuonErrorKind::Auth
            | MuonErrorKind::Send
            | MuonErrorKind::Closed
            | MuonErrorKind::Req
            | MuonErrorKind::Res => Self::NetworkError(e.to_string()),

            MuonErrorKind::Other => Self::UnknownError(e.to_string()),
        }
    }
}

impl From<muon::StatusErr> for ApiServiceError {
    fn from(e: muon::StatusErr) -> Self {
        let text = match String::from_utf8(e.1.body().to_owned()) {
            Ok(b) => b,
            Err(e) => return Self::Utf8DecodingError(e),
        };

        match (e.0, e.to_string()) {
            (s, e) if s.is_redirection() => Self::Redirect(e, text),
            (Status::BAD_REQUEST, e) => Self::BadRequest(e, text),
            (Status::UNAUTHORIZED, e) => Self::Unauthorized(e, text),
            (Status::NOT_FOUND, e) => Self::NotFound(e, text),
            (Status::UNPROCESSABLE_ENTITY, e) => Self::UnprocessableEntity(e, text),
            (Status::TOO_MANY_REQUESTS, e) => Self::TooManyRequest(e, text),
            (Status::INTERNAL_SERVER_ERROR, e) => Self::InternalServerError(e, text),
            (Status::NOT_IMPLEMENTED, e) => Self::NotImplemented(e, text),
            (Status::BAD_GATEWAY, e) => Self::BadGateway(e, text),
            (Status::SERVICE_UNAVAILABLE, e) => Self::ServiceUnavailable(e, text),
            (other, e) => Self::OtherHttpError(other, e, text),
        }
    }
}

/// Implements the muon store trait for our store type.
pub struct MuonStoreImpl<S> {
    env_id: EnvId,
    store: Arc<RwLock<S>>,
}

impl<S> MuonStoreImpl<S> {
    pub fn new(env_id: EnvId, store: Arc<RwLock<S>>) -> Self {
        Self { env_id, store }
    }
}

#[async_trait]
impl<S: Store + 'static> MuonStore for MuonStoreImpl<S> {
    fn env(&self) -> EnvId {
        self.env_id.clone()
    }

    async fn get_auth(&self) -> Auth {
        self.store.read().await.get_auth().await
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<Auth, MuonStoreError> {
        self.store
            .write()
            .await
            .set_auth(auth)
            .map_err(|_| MuonStoreError)
            .await?;

        Ok(self.get_auth().await)
    }
}

pub struct SetCryptoClockLayer;

impl SetCryptoClockLayer {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        let response = inner.send(req).await?;

        if let Some(date) = response
            .headers()
            .get("date")
            .and_then(|response_time_header| response_time_header.to_str().ok())
            .and_then(|response_time| DateTime::parse_from_rfc2822(response_time).ok())
            .and_then(|parsed_server_time| parsed_server_time.timestamp().try_into().ok())
            .map(UnixTimestamp)
        {
            server_crypto_clock().update_clock(date);
        }

        Ok(response)
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetCryptoClockLayer {
    fn on_send<'a: 'fut, 'fut>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'fut, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}

pub struct SetDefaultServiceTypeLayer;

impl SetDefaultServiceTypeLayer {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        let req = if req.get_service_type().is_none() {
            req.service_type(ServiceType::default(), true)
        } else {
            req
        };

        inner.send(req).await
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetDefaultServiceTypeLayer {
    fn on_send<'a: 'fut, 'fut>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'fut, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}

pub struct SetDefaultTimeoutLayer;

impl SetDefaultTimeoutLayer {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

        // NOTE: This is not a bug! Muon logs a warning if no timeout is explicitly set;
        // this workaround sets the timeout explicitly if it was not already set to a
        // non-default value earlier in the layer stack.
        let req = if req.get_allowed_time() == &DEFAULT_TIMEOUT {
            req.allowed_time(DEFAULT_TIMEOUT)
        } else {
            req
        };

        inner.send(req).await
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetDefaultTimeoutLayer {
    fn on_send<'a: 'fut, 'fut>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'fut, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}
