use std::error::Error;

use bytes::Bytes;
use muon::common::Timeout;
use muon::error::ErrorKind as MuonErrorKind;
use muon::util::ProtonRequestExt;
use muon::{serde_to_query, Status};
use muon::{GET, PUT};
use proton_crypto_account::keys::APIPublicAddressKeys;
use serde::Deserialize;

use crate::service::{ApiServiceError, ApiServiceResult};
use crate::services::proton::prelude::*;
use crate::services::proton::CORE_V4;
use crate::services::proton::{Proton, ProtonCore};

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
