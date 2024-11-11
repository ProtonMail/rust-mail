use bytes::Bytes;
use muon::serde_to_query;
use muon::util::ProtonRequestExt;
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

impl From<muon::StatusErr> for ApiServiceError {
    fn from(_: muon::StatusErr) -> Self {
        todo!()
    }
}

impl From<serde_qs::Error> for ApiServiceError {
    fn from(_: serde_qs::Error) -> Self {
        todo!()
    }
}
