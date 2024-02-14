use crate::http::{FromResponse, Result};
use serde::de::DeserializeOwned;
use std::marker::PhantomData;
use tracing::debug;

#[derive(Copy, Clone)]
pub struct NoResponse {}

impl FromResponse for NoResponse {
    type Output = ();

    const NEEDS_BODY: bool = false;

    fn from_response<T: AsRef<[u8]>>(_: T, _: bool) -> Result<Self::Output> {
        Ok(())
    }
}

pub struct JsonResponse<T: DeserializeOwned>(PhantomData<T>);

impl<T: DeserializeOwned> FromResponse for JsonResponse<T> {
    type Output = T;

    const NEEDS_BODY: bool = true;

    fn from_response<R: AsRef<[u8]>>(response: R, debug: bool) -> Result<Self::Output> {
        // uncomment for debug.
        if debug {
            debug!(
                "JsonResponse: {}",
                std::str::from_utf8(response.as_ref()).unwrap()
            );
        }
        let r = serde_json::from_slice(response.as_ref())?;
        Ok(r)
    }
}

#[derive(Copy, Clone)]
pub struct StringResponse {}

impl FromResponse for StringResponse {
    type Output = String;

    const NEEDS_BODY: bool = true;

    fn from_response<R: AsRef<[u8]>>(response: R, debug: bool) -> Result<Self::Output> {
        let v = String::from_utf8_lossy(response.as_ref()).to_string();
        if debug {
            debug!("StringResponse: {}", v);
        }
        Ok(String::from_utf8_lossy(response.as_ref()).to_string())
    }
}
