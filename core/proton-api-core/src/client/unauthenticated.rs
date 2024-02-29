use crate::http::{Client, HttpRequestError, RequestDesc};
use crate::requests::{CaptchaRequest, Ping};

pub async fn ping(client: &Client) -> Result<(), HttpRequestError> {
    client.execute_request(Ping {}.to_request()).await
}

pub async fn captcha_get(
    client: &Client,
    token: &str,
    force_web: bool,
) -> Result<String, HttpRequestError> {
    client
        .execute_request(CaptchaRequest::new(token, force_web).to_request())
        .await
}
