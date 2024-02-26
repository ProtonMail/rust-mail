use crate::clientv2::Session;
use crate::http;

#[derive(Debug, Clone)]
pub struct TotpSession(pub(super) Session);

impl TotpSession {
    pub async fn submit_totp<'a>(
        &'a self,
        code: &'a str,
    ) -> Result<Session, http::HttpRequestError> {
        self.0.submit_totp(code).await?;
        Ok(self.0.clone())
    }

    pub async fn logout(&self) -> Result<(), http::HttpRequestError> {
        self.0.logout().await
    }
}
