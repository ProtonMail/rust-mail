#![allow(clippy::module_name_repetitions)] // to avoid issue with collisions in the http namespace
use crate::http::{FromResponse, Result};
use base64::{engine::general_purpose, Engine as _};
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

pub struct ByteResponse {}

impl FromResponse for ByteResponse {
    type Output = Vec<u8>;

    const NEEDS_BODY: bool = true;

    fn from_response<R: AsRef<[u8]>>(response: R, debug: bool) -> Result<Self::Output> {
        let v: Vec<u8> = response.as_ref().to_vec();

        if debug {
            debug!("ByteResponse: {}", general_purpose::STANDARD.encode(&v));
        }

        Ok(v)
    }
}
