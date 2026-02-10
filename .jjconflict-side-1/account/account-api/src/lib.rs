#![allow(clippy::large_enum_variant)]
#![allow(clippy::result_large_err)] // TODO(ET-5588): address growing Error size

use crate::prelude::*;
use derive_more::Display;
use muon::{
    ProtonRequest, ProtonResponse, Status, common::Sender, http::HttpReqExt, serde_to_query,
};
use proton_core_api::services::observability::ApiServiceObservabilityResponse;
use serde::Deserialize;
use serde_json::Value;

#[macro_use]
extern crate tracing;

#[macro_use]
extern crate muon;

pub mod countries;
pub mod login;
pub mod password;
pub mod prelude;
pub mod requests;
pub mod responses;
pub mod shared;
pub mod signup;

/// The Proton Core API base path (v4).
pub const CORE_V4: &str = "/core/v4";
pub const AUTH_V4: &str = "/auth/v4";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Serialization(#[from] serde_qs::Error),

    #[error(transparent)]
    Muon(#[from] muon::Error),

    #[error(transparent)]
    Status(#[from] muon::StatusErr),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Additional information about an API error.
///
/// If a response is received with an HTTP status code that indicates a protocol
/// error, then it may be accompanied by additional information about the error.
/// This struct provides a way to access that information.
///
#[derive(Clone, Debug, Display, Default, Deserialize, Eq, PartialEq)]
#[display("{code}: {error:?} ({details:?})")]
#[serde(rename_all = "PascalCase")]
pub struct ApiErrorInfo {
    /// Internal API code.
    pub code: u32,

    /// Optional error message that may be present.
    pub error: Option<String>,

    /// Optional JSON type with error details.
    pub details: Option<Value>,
}

impl ApiError {
    #[must_use]
    pub fn err_status(&self) -> Option<Status> {
        if let Self::Status(muon::StatusErr(code, _)) = self {
            Some(code.to_owned())
        } else {
            None
        }
    }

    #[must_use]
    pub fn err_code(&self) -> Option<u32> {
        Some(self.err_info()?.code)
    }

    #[must_use]
    pub fn err_info(&self) -> Option<ApiErrorInfo> {
        if let Self::Status(muon::StatusErr(_, res)) = self {
            serde_json::from_str(res.body_str().ok()?).ok()
        } else {
            None
        }
    }

    #[must_use]
    pub fn body_str(&self) -> Option<&str> {
        if let Self::Status(muon::StatusErr(_, res)) = self {
            res.body_str().ok()
        } else {
            None
        }
    }

    #[must_use]
    pub fn is_network_failure(&self) -> bool {
        matches!(self, ApiError::Muon(_))
    }
}

impl From<&ApiError> for ApiServiceObservabilityResponse {
    fn from(value: &ApiError) -> Self {
        match value {
            ApiError::Serialization(_) => ApiServiceObservabilityResponse::SerializationError,
            ApiError::Muon(_) => ApiServiceObservabilityResponse::NetworkError,
            ApiError::Status(status_err) => {
                if status_err.0.is_client_error() {
                    ApiServiceObservabilityResponse::Http4xx
                } else if status_err.0.is_server_error() {
                    ApiServiceObservabilityResponse::Http5xx
                } else {
                    ApiServiceObservabilityResponse::NetworkError
                }
            }
            ApiError::Internal(_) => ApiServiceObservabilityResponse::Unknown,
        }
    }
}

/// A result containing an error that defaults to `ApiServiceError`.
pub type ApiServiceResult<T, E = ApiError> = Result<T, E>;

#[allow(async_fn_in_trait)]
pub trait AccountApi {
    async fn get_password_policies(&self) -> ApiServiceResult<GetPasswordPoliciesResponse>;

    /// Get a new random auth modulus.
    async fn get_auth_modulus(&self) -> ApiServiceResult<GetAuthModulusResponse>;

    /// Get the available addresses in the account.
    async fn get_addresses(&self) -> ApiServiceResult<GetAddressesResponse>;

    /// Retrieves a list of available domains.
    ///
    /// This method queries the available domains for email address creation, optionally filtered
    /// by domain type. See [API docs](https://protonmail.gitlab-pages.protontech.ch/Slim-API/account/#tag/Domains/operation/get_core-%7B_version%7D-domains-available)
    /// for more details.
    ///
    /// # Arguments
    /// * `domain_type` - An optional filter for the type of domains to retrieve (e.g., "custom").
    ///
    /// # Returns
    /// An `ApiServiceResult` containing the list of available domains or an error.
    ///
    /// [API doc](https://protonmail.gitlab-pages.protontech.ch/Slim-API/account/#tag/Domains/operation/get_core-%7B_version%7D-domains-available)
    async fn get_available_domains(
        &self,
        domain_type: Option<String>,
    ) -> ApiServiceResult<GetAvailableDomainsResponse>;

    /// Checks the availability of a username.
    ///
    /// This method verifies if a given username is available for use, with an option to parse it
    /// as a full email address and include payment information.
    ///
    /// # Arguments
    /// * `name` - The username to check.
    /// * `parse_domain` - Indicates whether to parse the username as a full email address.
    /// * `payment_info_token` - An optional token for payment-related validation.
    ///
    /// # Returns
    /// An `ApiServiceResult` containing a response code indicating availability or an error
    ///
    /// [API doc](https://protonmail.gitlab-pages.protontech.ch/Slim-API/core/#tag/Users/operation/get_core-%7B_version%7D-users-available)
    async fn check_username_availability(
        &self,
        name: String,
        parse_domain: ParseDomain,
        payment_info_token: Option<&str>,
    ) -> ApiServiceResult<SimpleResponse>;

    /// Checks the availability of an external username.
    ///
    /// This method verifies if an external username is available, with an optional payment token.
    ///
    /// # Arguments
    /// * `name` - The external username to check.
    /// * `payment_info_token` - An optional token for payment-related validation.
    ///
    /// # Returns
    /// An `ApiServiceResult` containing a response code indicating availability or an error.
    ///
    /// [API doc](https://protonmail.gitlab-pages.protontech.ch/Slim-API/core/#tag/Users/operation/get_core-%7B_version%7D-users-availableExternal)
    async fn check_external_username_availability(
        &self,
        name: String,
        payment_info_token: Option<&str>,
    ) -> ApiServiceResult<SimpleResponse>;

    /// Sends a verification code to a user.
    ///
    /// This method requests a verification code to be sent via email or SMS, based on the provided
    /// request details.
    ///
    /// # Arguments
    /// * `request` - The request specifying the verification method and destination.
    ///
    /// # Returns
    /// An `ApiServiceResult` containing a response code indicating success or an error.
    ///
    /// [API doc](https://protonmail.gitlab-pages.protontech.ch/Slim-API/core/#tag/Users/operation/post_core-%7B_version%7D-users-code)
    async fn send_verification_code(
        &self,
        request: SendVerificationCodeRequest,
    ) -> ApiServiceResult<SimpleResponse>;

    /// Creates a new user account.
    ///
    /// ...
    async fn create_user(&self, request: CreateUserRequest)
    -> ApiServiceResult<CreateUserResponse>;

    /// Creates a new external user account.
    ///
    /// ...
    async fn create_external_user(
        &self,
        request: CreateExternalUserRequest,
    ) -> ApiServiceResult<CreateUserResponse>;

    /// Performs the initial key setup for new private users.
    ///
    /// This method sets up encryption keys for a new private user account, including user initialization
    /// flags and key details.
    ///
    /// # Arguments
    /// * `user_init_flag` - Flag indicating that /core/v4/welcome-mail-send and /core/v4/checklist/get-started/init endpoints are called by the client.
    /// * `request` - The request containing key setup details.
    ///
    /// # Returns
    /// An `ApiServiceResult` containing the setup response with user and key details or an error.
    ///
    /// [API doc](https://protonmail.gitlab-pages.protontech.ch/Slim-API/core/#tag/Keys/operation/post_core-%7B_version%7D-keys-setup)
    async fn setup_keys(
        &self,
        user_init_flag: AsyncUserInitialization,
        request: SetupKeysRequest,
    ) -> ApiServiceResult<SetupKeysResponse>;

    /// Sets up a new address for a non-subscriber user.
    ///
    /// This method sends a request to create a new email address for a non-subscriber user,
    /// returning the result of the operation.
    ///
    /// # Arguments
    /// * `request` - The request containing details for the new address setup.
    ///
    /// # Returns
    /// An `ApiServiceResult` containing the response with the created address details or an error.
    ///
    /// [API doc](https://protonmail.gitlab-pages.protontech.ch/Slim-API/core/#tag/Address/operation/post_core-%7B_version%7D-addresses-setup)
    async fn setup_address(
        &self,
        request: PostAddressesSetupRequest,
    ) -> ApiServiceResult<PostAddressesSetupResponse>;

    /// Checks if the provided email address is valid
    /// [API doc](https://proton.black/api/internal/doc?page=core#tag/Validation/operation/post_core-{_version}-validate-email)
    async fn validate_email(
        &self,
        request: ValidateEmailRequest,
    ) -> ApiServiceResult<SimpleResponse>;

    /// Checks if the provided phone number is valid
    /// [API doc](https://proton.black/api/internal/doc?page=core#tag/Validation/operation/post_core-{_version}-validate-phone)
    async fn validate_phone(
        &self,
        request: ValidatePhoneRequest,
    ) -> ApiServiceResult<SimpleResponse>;

    /// Creates a new user key.
    ///
    /// This method sends a request to create a new encryption key for the user.
    ///
    /// # Arguments
    /// * `request` - The request containing the private key and primary flag.
    ///
    /// # Returns
    /// An `ApiServiceResult` containing the response with the created key ID or an error.
    ///
    /// [API doc](https://protonmail.gitlab-pages.protontech.ch/Slim-API/core/#tag/Keys/operation/post_core-{_version}-keys-user)
    async fn create_user_key(
        &self,
        request: CreateUserKeyRequest,
    ) -> ApiServiceResult<CreateUserKeyResponse>;

    /// Creates a new address key.
    ///
    /// This method sends a request to create a new encryption key for an address.
    ///
    /// # Arguments
    /// * `request` - The request containing the address key details.
    ///
    /// # Returns
    /// An `ApiServiceResult` containing the response with the created key details or an error.
    ///
    /// [API doc](https://protonmail.gitlab-pages.protontech.ch/Slim-API/core/#tag/Keys/operation/post_core-{_version}-keys-address)
    async fn create_address_key(
        &self,
        request: CreateAddressKeyRequest,
    ) -> ApiServiceResult<CreateAddressKeyResponse>;

    async fn put_settings_password(
        &self,
        body: PutSettingsPasswordRequest,
    ) -> ApiServiceResult<PutSettingsPasswordResponse>;

    async fn put_keys_private(
        &self,
        body: PutKeysPrivateRequest,
    ) -> ApiServiceResult<PutKeysPrivateResponse>;

    async fn put_users_password(
        &self,
        body: PutUsersPasswordRequest,
    ) -> ApiServiceResult<PutUsersPasswordResponse>;
}

impl<This: ?Sized + Sender<ProtonRequest, ProtonResponse>> AccountApi for This {
    async fn get_password_policies(&self) -> ApiServiceResult<GetPasswordPoliciesResponse> {
        Ok(GET!("{AUTH_V4}/password-policies")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_auth_modulus(&self) -> ApiServiceResult<GetAuthModulusResponse> {
        Ok(GET!("{AUTH_V4}/modulus")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_addresses(&self) -> ApiServiceResult<GetAddressesResponse> {
        Ok(GET!("{CORE_V4}/addresses")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_available_domains(
        &self,
        domain_type: Option<String>,
    ) -> ApiServiceResult<GetAvailableDomainsResponse> {
        Ok(GET!("{CORE_V4}/domains/available")
            .query(serde_to_query(GetAvailableDomainsRequest { domain_type })?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn check_username_availability(
        &self,
        name: String,
        parse_domain: ParseDomain,
        payment_info_token: Option<&str>,
    ) -> ApiServiceResult<SimpleResponse> {
        let mut request = GET!("{CORE_V4}/users/available")
            .query(serde_to_query(CheckUsernameRequest { name, parse_domain })?);
        request = add_payment_header(request, payment_info_token);
        Ok(request.send_with(self).await?.ok()?.into_body_json()?)
    }

    async fn check_external_username_availability(
        &self,
        name: String,
        payment_info_token: Option<&str>,
    ) -> ApiServiceResult<SimpleResponse> {
        let mut request = GET!("{CORE_V4}/users/availableExternal")
            .query(serde_to_query(CheckExternalUsernameRequest { name })?);
        request = add_payment_header(request, payment_info_token);
        Ok(request.send_with(self).await?.ok()?.into_body_json()?)
    }

    async fn setup_address(
        &self,
        request: PostAddressesSetupRequest,
    ) -> ApiServiceResult<PostAddressesSetupResponse> {
        Ok(POST!("{CORE_V4}/addresses/setup")
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn send_verification_code(
        &self,
        request: SendVerificationCodeRequest,
    ) -> ApiServiceResult<SimpleResponse> {
        Ok(POST!("{CORE_V4}/users/code")
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn create_user(
        &self,
        request: CreateUserRequest,
    ) -> ApiServiceResult<CreateUserResponse> {
        Ok(POST!("{CORE_V4}/users")
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn create_external_user(
        &self,
        request: CreateExternalUserRequest,
    ) -> ApiServiceResult<CreateUserResponse> {
        Ok(POST!("{CORE_V4}/users/external")
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn setup_keys(
        &self,
        user_init_flag: AsyncUserInitialization,
        request: SetupKeysRequest,
    ) -> ApiServiceResult<SetupKeysResponse> {
        let user_init_flag: i32 = user_init_flag.into();
        Ok(POST!("{CORE_V4}/keys/setup")
            .query(serde_to_query(("AsyncUserInitialization", user_init_flag))?)
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn validate_email(
        &self,
        request: ValidateEmailRequest,
    ) -> ApiServiceResult<SimpleResponse> {
        // We need an unauth session for this call so we the request through the client send function.
        Ok(self
            .send(POST!("{CORE_V4}/validate/email").body_json(request)?)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn validate_phone(
        &self,
        request: ValidatePhoneRequest,
    ) -> ApiServiceResult<SimpleResponse> {
        // We need an unauth session for this call so we the request through the client send function.
        Ok(self
            .send(POST!("{CORE_V4}/validate/phone").body_json(request)?)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn create_user_key(
        &self,
        request: CreateUserKeyRequest,
    ) -> ApiServiceResult<CreateUserKeyResponse> {
        Ok(POST!("{CORE_V4}/keys/user")
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn create_address_key(
        &self,
        request: CreateAddressKeyRequest,
    ) -> ApiServiceResult<CreateAddressKeyResponse> {
        Ok(POST!("{CORE_V4}/keys/address")
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_settings_password(
        &self,
        body: PutSettingsPasswordRequest,
    ) -> ApiServiceResult<PutSettingsPasswordResponse> {
        Ok(PUT!("{CORE_V4}/settings/password")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_keys_private(
        &self,
        body: PutKeysPrivateRequest,
    ) -> ApiServiceResult<PutKeysPrivateResponse> {
        Ok(PUT!("{CORE_V4}/keys/private")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_users_password(
        &self,
        body: PutUsersPasswordRequest,
    ) -> ApiServiceResult<PutUsersPasswordResponse> {
        Ok(PUT!("{CORE_V4}/users/password")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }
}

fn add_payment_header(request: muon::ProtonRequest, token: Option<&str>) -> muon::ProtonRequest {
    if let Some(token) = token {
        request.header(("X-PM-Payment-Info-Token", token))
    } else {
        request
    }
}
