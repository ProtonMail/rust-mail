#![allow(clippy::module_name_repetitions)] // to avoid issue with collisions in the http namespace
use crate::http::{ClientRequestBuilder, FromResponse, Method};
use bytes::Bytes;
use serde::Serialize;
use std::collections::HashMap;
use std::marker::PhantomData;

/// HTTP Request representation.
#[derive(Debug, Clone)]
pub struct RequestData {
    #[allow(unused)] // Only used by http implementations.
    pub(super) method: Method,
    #[allow(unused)] // Only used by http implementations.
    pub(super) url: String,
    pub(super) headers: HashMap<String, String>,
    pub(super) body: Option<Bytes>,
    pub(super) queries: Vec<(String, String)>,
}

impl RequestData {
    #[must_use]
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HashMap::new(),
            body: None,
            queries: Vec::new(),
        }
    }

    /// Set an http header and its value.
    #[must_use]
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set an http bearer token.
    #[must_use]
    pub fn bearer_token(self, token: impl AsRef<str>) -> Self {
        self.header("authorization", format!("Bearer {}", token.as_ref()))
    }

    /// Set the body from a collection of bytes.
    #[must_use]
    pub fn bytes(mut self, bytes: impl Into<Bytes>) -> Self {
        self.body = Some(bytes.into());
        self
    }

    /// Set the body from a type that will be serialized as JSON.
    /// # Panics
    /// Will panic if the type can not be serialized.
    #[must_use]
    pub fn json(self, value: impl Serialize) -> Self {
        let bytes = serde_json::to_vec(&value).expect("Failed to serialize json");
        self.json_bytes(bytes)
    }

    /// Set the body from a collection of bytes which compromise a json object.
    #[must_use]
    pub fn json_bytes(mut self, bytes: impl Into<Bytes>) -> Self {
        self.body = Some(bytes.into());
        self.header("Content-Type", "application/json")
    }

    /// Set an http URL query parameter.
    #[must_use]
    pub fn query(mut self, key: impl Into<String>, value: &impl ToString) -> Self {
        self.queries.push((key.into(), value.to_string()));
        self
    }

    /// Set an http URL array query parameter.
    #[must_use]
    pub fn query_array(
        mut self,
        key: impl Into<String>,
        value: impl IntoIterator<Item = impl ToString>,
    ) -> Self {
        let mut key = key.into();
        key.push_str("[]");
        for v in value {
            self.queries.push((key.clone(), v.to_string()));
        }
        self
    }
}

pub trait RequestDesc {
    type Response: FromResponse;

    fn build(&self) -> RequestData;
    fn to_request(&self) -> OwnedRequest<Self::Response> {
        OwnedRequest(self.build(), PhantomData)
    }
}

pub struct OwnedRequest<F: FromResponse>(RequestData, PhantomData<F>);

impl<F: FromResponse> OwnedRequest<F> {
    pub fn new(r: RequestData) -> Self {
        Self(r, PhantomData)
    }
}

impl<R: RequestDesc> From<R> for OwnedRequest<R::Response> {
    fn from(value: R) -> Self {
        Self(value.build(), PhantomData)
    }
}

impl<F: FromResponse> Request for OwnedRequest<F> {
    type Response = F;

    fn build<C: ClientRequestBuilder>(&self, builder: &C) -> C::Request {
        builder.new_request(&self.0)
    }
}

pub trait Request {
    type Response: FromResponse;

    fn build<C: ClientRequestBuilder>(&self, builder: &C) -> C::Request;
}
