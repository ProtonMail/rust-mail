use crate::domain::TwoFactorAuth;
use crate::http;
use crate::LoginError;
use std::sync::Arc;
use uniffi;
#[derive(uniffi::Error, Debug, thiserror::Error)]
#[uniffi(flat_error)]
pub enum ClientError {
    #[error("{0}")]
    Error(#[from] anyhow::Error),
}

#[derive(uniffi::Object)]
pub struct Client(pub http::Client);

#[uniffi::export]
impl Client {
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, ClientError> {
        let c = crate::http::ClientBuilder::new()
            .app_version("Other")
            .build()?;
        Ok(Arc::new(Self(c)))
    }
}

#[derive(uniffi::Object)]
pub struct Session(pub crate::Session);

#[uniffi::export(async_runtime = "tokio")]
impl Session {
    pub async fn logout(&self) -> Result<(), http::HttpRequestError> {
        self.0.logout().await
    }
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn session_login(
    client: &Client,
    email: String,
    password: String,
) -> Result<Arc<Session>, LoginError> {
    let crate::SessionType::Authenticated(session) =
        crate::Session::login(client.0.clone(), &email, &password, None).await?
    else {
        return Err(LoginError::Unsupported2FA(TwoFactorAuth::TOTP));
    };

    Ok(Arc::new(Session(session)))
}
