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
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HashMap::new(),
            body: None,
            queries: Vec::new(),
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn bearer_token(self, token: impl AsRef<str>) -> Self {
        self.header("authorization", format!("Bearer {}", token.as_ref()))
    }

    pub fn bytes(mut self, bytes: impl Into<Bytes>) -> Self {
        self.body = Some(bytes.into());
        self
    }

    pub fn json(self, value: impl Serialize) -> Self {
        let bytes = serde_json::to_vec(&value).expect("Failed to serialize json");
        self.json_bytes(bytes)
    }

    pub fn json_bytes(mut self, bytes: impl Into<Bytes>) -> Self {
        self.body = Some(bytes.into());
        self.header("Content-Type", "application/json")
    }

    pub fn query(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.queries.push((key.into(), value.to_string()));
        self
    }

    pub fn query_array(
        mut self,
        key: impl Into<String>,
        value: impl IntoIterator<Item = impl ToString>,
    ) -> Self {
        let mut key = key.into();
        key.push_str("[]");
        for v in value.into_iter() {
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
