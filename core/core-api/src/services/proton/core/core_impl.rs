use std::io::Cursor;
use std::time::Duration;

use bytes::Bytes;
use muon::common::{RetryPolicy, Sender};
use muon::util::ProtonRequestExt;
use muon::{DELETE, GET, PATCH, POST, PUT};
use muon::{ProtonRequest, ProtonResponse, serde_to_query};
use proton_crypto_account::keys::APIPublicAddressKeys;
use serde_json::json;

use crate::service::ApiServiceResult;
use crate::services::proton::core::{CORE_V4, CORE_V5, ProtonCore};
use crate::services::proton::prelude::*;
use crate::utils::HttpReqExt;

impl<This: ?Sized + Sender<ProtonRequest, ProtonResponse>> ProtonCore for This {
    async fn get_addresses(&self) -> ApiServiceResult<GetAddressesResponse> {
        Ok(GET!("{CORE_V4}/addresses")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_address_by_id(&self, id: AddressId) -> ApiServiceResult<GetAddressResponse> {
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

    async fn get_contact(&self, contact_id: ContactId) -> ApiServiceResult<GetContactResponse> {
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
        ids: Vec<ContactId>,
    ) -> ApiServiceResult<PutDeleteContactsResponse> {
        Ok(PUT!("/contacts/delete")
            .body_json(PutDeleteContacts { ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    // Event APIs
    // https://protonmail.gitlab-pages.protontech.ch/Slim-API/core/#tag/Events

    async fn get_event(
        &self,
        event_id: EventId,
        options: GetEventOptions,
    ) -> ApiServiceResult<String> {
        Ok(GET!("{CORE_V5}/events/{event_id}")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_string()?)
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

    async fn get_tests_ping(
        &self,
        timeout: Option<Duration>,
        retry: Option<RetryPolicy>,
    ) -> ApiServiceResult<()> {
        GET!("{CORE_V4}/tests/ping")
            .with_allowed_time(timeout)
            .with_retry_policy(retry)
            .send_with(self)
            .await?
            .ok()?;

        Ok(())
    }

    async fn get_users(&self) -> ApiServiceResult<GetUsersResponse> {
        Ok(GET!("{CORE_V4}/users")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn delete_label(&self, label_id: LabelId) -> ApiServiceResult<()> {
        DELETE!("{CORE_V4}/labels/{label_id}")
            .send_with(self)
            .await?
            .ok()?;

        Ok(())
    }

    async fn get_labels(&self, label_type: LabelType) -> ApiServiceResult<GetLabelsResponse> {
        Ok(GET!("{CORE_V4}/labels")
            .query(serde_to_query(GetLabelsOptions { label_type })?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_labels_by_ids(
        &self,
        label_ids: Vec<LabelId>,
    ) -> ApiServiceResult<GetLabelsResponse> {
        Ok(POST!("{CORE_V4}/labels/by-ids")
            .body_json(GetLabelsByIdsOptions { label_ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn post_labels(&self, body: PostLabelsRequest) -> ApiServiceResult<PostLabelsResponse> {
        Ok(POST!("{CORE_V4}/labels")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_label(
        &self,
        label_id: LabelId,
        body: PutLabelRequest,
    ) -> ApiServiceResult<PutLabelResponse> {
        Ok(PUT!("{CORE_V4}/labels/{label_id}")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn patch_label(
        &self,
        label_id: LabelId,
        body: PatchLabelRequest,
    ) -> ApiServiceResult<PatchLabelResponse> {
        Ok(PATCH!("{CORE_V4}/labels/{label_id}")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn register_device(&self, body: RegisterDeviceRequest) -> ApiServiceResult<()> {
        POST!("{CORE_V4}/devices")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?;

        Ok(())
    }

    async fn post_report_bug(&self, body: PostReportBug) -> ApiServiceResult<()> {
        POST!("{CORE_V4}/reports/bug")
            .multipart(move |mut form| {
                form.add_text("OS", body.os);
                form.add_text("OSVersion", body.os_version);
                form.add_text("Client", body.client);
                form.add_text("ClientVersion", body.client_version);
                form.add_text("ClientType", body.client_type.to_string());
                form.add_text("Title", body.title);
                form.add_text("Description", body.description);
                form.add_text("Username", body.username);
                form.add_text("Email", body.email);
                if let Some((file_name, logs)) = body.logs {
                    form.add_reader_file_with_mime(
                        "ApplicationLogs",
                        Cursor::new(logs),
                        file_name,
                        "application/zip"
                            .parse()
                            .expect("Somehow application/zip MIME left the earth"),
                    );
                }
                form
            })
            .await?
            .send_with(self)
            .await?
            .ok()?;

        Ok(())
    }

    async fn proxy_img(&self, url: &url::Url) -> ApiServiceResult<Vec<u8>> {
        let query = json! ({
            "Url": url
        });

        Ok(GET!("{CORE_V4}/images")
            .query(serde_to_query(query)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body())
    }

    async fn get_unleash_feature_flags(&self) -> ApiServiceResult<GetUnleashFeaturesResponse> {
        Ok(GET!("{UNLEASH_V2}/frontend")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_legacy_feature_flags(
        &self,
        options: GetLegacyFeatureFlagsOptions,
    ) -> ApiServiceResult<GetLegacyFeaturesResponse> {
        Ok(GET!("{CORE_V4}/features")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }
}
