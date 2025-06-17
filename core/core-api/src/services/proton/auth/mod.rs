mod auth_impl;
mod responses;

pub use self::responses::*;
use crate::service::ApiServiceResult;

/// The Proton Auth API base path (v4).
pub const AUTH_V4: &str = "/auth/v4";

#[allow(async_fn_in_trait)]
pub trait ProtonAuth {
    /// GET the user's session UUID.
    async fn get_sessions_uuid(&self) -> ApiServiceResult<GetSessionsUuidResponse>;
}
