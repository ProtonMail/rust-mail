use crate::service::ApiServiceResult;

export! {
    mod common (as pub);
    mod request_data (as pub);
    mod requests (as pub);
    mod response_data (as pub);
    mod responses (as pub);
}

mod auth_impl;

/// The Proton Auth API base path (v4).
pub const AUTH_V4: &str = "/auth/v4";

#[allow(async_fn_in_trait)]
pub trait ProtonAuth {
    /// GET the user's session UUID.
    async fn get_sessions_uuid(&self) -> ApiServiceResult<GetSessionsUuidResponse>;
}
