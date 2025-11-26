mod common;
mod core_impl;
mod request_data;
mod requests;
mod response_data;
mod responses;

pub use self::common::*;
pub use self::request_data::*;
pub use self::requests::*;
pub use self::response_data::*;
pub use self::responses::*;
use crate::service::ApiServiceResult;
use bytes::Bytes;
use muon::common::RetryPolicy;
use proton_crypto_account::keys::APIPublicAddressKeys;
use std::future::Future;
use std::time::Duration;

/// The Proton Core API base path (v4).
pub const CORE_V4: &str = "/core/v4";

/// The Proton Core API base path (v5).
pub const CORE_V5: &str = "/core/v5";

/// The Unleash API base path (v2).
pub const UNLEASH_V2: &str = "/feature/v2";

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
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_captcha(&self, options: GetCaptchaOptions) -> ApiServiceResult<String>;

    /// GETs a single contact.
    ///
    /// This returns the full contact record.
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
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_event(
        &self,
        event_id: EventId,
        options: GetEventOptions,
    ) -> ApiServiceResult<String>;

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
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn delete_label(&self, label_id: LabelId) -> ApiServiceResult<()>;

    /// Method requests all labels with given label type
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
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn post_labels(&self, body: PostLabelsRequest) -> ApiServiceResult<PostLabelsResponse>;

    /// TODO: Document this method.
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
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn register_device(&self, body: RegisterDeviceRequest) -> ApiServiceResult<()>;

    /// This method allows to create a ticket for bug in API (and in zendesk)
    /// for support team to review issue reported by a user.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn post_report_bug(&self, body: PostReportBug) -> ApiServiceResult<()>;

    /// Gets an image through proton's proxy.
    async fn proxy_img(&self, url: &url::Url) -> ApiServiceResult<Vec<u8>>;

    /// Gets feature flags defined in Unleash service.
    /// See: <https://docs.getunleash.io/reference/api/unleash/get-frontend-features/>
    async fn get_unleash_feature_flags(&self) -> ApiServiceResult<GetUnleashFeaturesResponse>;

    /// Gets feature flags defined in our own legacy service.
    async fn get_legacy_feature_flags(
        &self,
        options: GetLegacyFeatureFlagsOptions,
    ) -> ApiServiceResult<GetLegacyFeaturesResponse>;
}
