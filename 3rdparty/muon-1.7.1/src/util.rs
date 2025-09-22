//! ## Util
//!
//! Just some useful stuff.

use crate::common::BoxErr;
pub use crate::http::HttpReqExt as ProtonRequestExt;
use async_trait::async_trait;
use data_encoding::{DecodeError, BASE32_NOPAD, BASE64, BASE64URL_NOPAD};
use futures::prelude::*;
use futures::TryFutureExt;
use itertools::Itertools;
use muon_proc::autoimpl;
use num::cast::AsPrimitive;
use pin_project::pin_project;
use serde::de::Deserialize;
use serde_json::Error as JsonError;
use sha2::{Digest, Sha256};
use std::array::TryFromSliceError;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::hash::Hash;
use std::iter::once;
use std::marker::PhantomData;
use std::pin::Pin;
use std::string::FromUtf8Error;
use std::task::{ready, Context, Poll};
use std::time::Duration;
use thiserror::Error;

/// A decoding error.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum ByteSliceErr {
    /// A data decoding error.
    Data(#[from] DecodeError),

    /// A UTF-8 decoding error.
    Utf8(#[from] FromUtf8Error),

    /// A JSON encoding/decoding error.
    Json(#[from] JsonError),

    /// A slice conversion error.
    Slice(#[from] TryFromSliceError),
}

impl From<Infallible> for ByteSliceErr {
    fn from(err: Infallible) -> Self {
        match err {}
    }
}

/// Extends byte slices with various encoding/decoding/hashing methods.
#[autoimpl]
pub trait ByteSliceExt: AsRef<[u8]> {
    /// Encodes the value as base32.
    fn as_b32(&self) -> String {
        BASE32_NOPAD.encode(self.as_ref())
    }

    /// Decodes the value from base32.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not valid base32.
    fn b32_into<T>(&self) -> Result<T, ByteSliceErr>
    where
        for<'a> &'a [u8]: TryInto<T>,
        for<'a> <&'a [u8] as TryInto<T>>::Error: Into<ByteSliceErr>,
    {
        (BASE32_NOPAD.decode(self.as_ref())?)
            .as_slice()
            .try_into()
            .err_into()
    }

    /// Decodes the value from base32 and converts to UTF-8.
    fn b32_to_string(&self) -> Result<String, ByteSliceErr> {
        self.b32_into::<Vec<u8>>().and_then(|s| s.as_utf8())
    }

    /// Encodes the value as base64.
    fn as_b64(&self) -> String {
        BASE64.encode(self.as_ref())
    }

    /// Encodes the value as URL-safe, no-padding base64.
    fn as_b64_url(&self) -> String {
        BASE64URL_NOPAD.encode(self.as_ref())
    }

    /// Decodes the value from base64.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not valid base64.
    fn b64_into<T>(&self) -> Result<T, ByteSliceErr>
    where
        for<'a> &'a [u8]: TryInto<T>,
        for<'a> <&'a [u8] as TryInto<T>>::Error: Into<ByteSliceErr>,
    {
        (BASE64.decode(self.as_ref())?)
            .as_slice()
            .try_into()
            .err_into()
    }

    /// Decodes the value from URL-safe, no-padding base64.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not valid base64.
    fn b64_url_into<T>(&self) -> Result<T, ByteSliceErr>
    where
        for<'a> &'a [u8]: TryInto<T>,
        for<'a> <&'a [u8] as TryInto<T>>::Error: Into<ByteSliceErr>,
    {
        (BASE64URL_NOPAD.decode(self.as_ref())?)
            .as_slice()
            .try_into()
            .err_into()
    }

    /// Decodes the value from base64 and converts to UTF-8.
    fn b64_to_string(&self) -> Result<String, ByteSliceErr> {
        self.b64_into::<Vec<u8>>().and_then(|s| s.as_utf8())
    }

    /// Computes the SHA-256 hash of the value.
    fn sha256(&self) -> [u8; 32] {
        Sha256::digest(self.as_ref()).into()
    }

    /// Convert this to a string as UTF-8.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not valid UTF-8.
    fn as_utf8(&self) -> Result<String, ByteSliceErr> {
        String::from_utf8(self.as_ref().into()).err_into()
    }

    /// Convert this to a string as UTF-8 lossy.
    fn as_utf8_lossy(&self) -> String {
        String::from_utf8_lossy(self.as_ref()).into()
    }

    /// Encode this slice as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be encoded as JSON.
    fn as_json(&self) -> Result<Vec<u8>, ByteSliceErr> {
        serde_json::to_vec(self.as_ref()).err_into()
    }

    /// Decode a value from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be decoded from JSON.
    fn json_into<T>(&self) -> Result<T, ByteSliceErr>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_slice(self.as_ref()).err_into()
    }
}

/// Trait for working with `Duration`.
///
/// Enables converting numeric types to and from `Duration`.
#[autoimpl]
pub trait DurationExt: AsPrimitive<u64> {
    /// Treats the value as `minutes`.
    fn m(self) -> Duration {
        Duration::from_secs(self.as_() * 60)
    }

    /// Treats the value as `seconds`.
    fn s(self) -> Duration {
        Duration::from_secs(self.as_())
    }

    /// Treats the value as `milliseconds`.
    fn ms(self) -> Duration {
        Duration::from_millis(self.as_())
    }

    /// Treats the value as `microseconds`.
    fn us(self) -> Duration {
        Duration::from_micros(self.as_())
    }

    /// Treats the value as `nanoseconds`.
    fn ns(self) -> Duration {
        Duration::from_nanos(self.as_())
    }
}

/// Converts a string into a vector of relative URL path segments.
#[autoimpl]
pub trait IntoPathSegs: AsRef<str> {
    /// Converts the string into a vector of path segments.
    fn as_path_segs(&self) -> Vec<String> {
        match self.as_ref().split('/').into_head_tail() {
            Some(("", tail)) => tail.map_into().into_vec(),
            Some((head, tail)) => once(head).chain(tail).map_into().into_vec(),
            None => Vec::new(),
        }
    }
}

/// Extends iterators with various methods.
#[autoimpl]
pub trait IntoIterExt: IntoIterator + Sized {
    /// Collects the iterator into a `Vec`.
    fn into_vec<T>(self) -> Vec<T>
    where
        Self: IntoIterator<Item = T>,
    {
        self.into_iter().collect()
    }

    /// Tries to collect the iterator into a `Result<Vec<T>, E>`.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the items fail to convert.
    fn try_into_vec<T, E>(self) -> Result<Vec<T>, E>
    where
        Self: IntoIterator<Item = Result<T, E>>,
    {
        self.into_iter().collect::<Result<Vec<T>, E>>()
    }

    /// Collects the iterator into a `HashMap`.
    fn into_map<K: Hash + Eq, V>(self) -> HashMap<K, V>
    where
        Self: IntoIterator<Item = (K, V)>,
    {
        self.into_iter().collect()
    }

    /// Tries to collect the iterator into a `Result<HashMap<K, V>, E>`.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the items fail to convert.
    fn try_into_map<K: Hash + Eq, V, E>(self) -> Result<HashMap<K, V>, E>
    where
        Self: IntoIterator<Item = Result<(K, V), E>>,
    {
        self.into_iter().collect::<Result<HashMap<K, V>, E>>()
    }

    /// Collects the iterator into a `HashSet`.
    fn into_set<T: Hash + Eq>(self) -> HashSet<T>
    where
        Self: IntoIterator<Item = T>,
    {
        self.into_iter().collect()
    }

    /// Tries to collect the iterator into a `Result<HashSet<T>, E>`.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the items fail to convert.
    fn try_into_set<T: Hash + Eq, E>(self) -> Result<HashSet<T>, E>
    where
        Self: IntoIterator<Item = Result<T, E>>,
    {
        self.into_iter().collect::<Result<HashSet<T>, E>>()
    }

    /// Collects the iterator into a head and a tail, or `None` if empty.
    fn into_head_tail(self) -> Option<(Self::Item, Self::IntoIter)> {
        let mut this = self.into_iter();

        let head = this.next()?;

        Some((head, this))
    }
}

/// Extends a `Result<T, E>` with various methods.
#[autoimpl]
pub trait ResultExt<T, E>: Into<Result<T, E>> + Sized {
    /// Maps the success of the result using `Into::into`.
    ///
    /// # Errors
    ///
    /// ... self explanatory
    fn ok_into<U>(self) -> Result<U, E>
    where
        T: Into<U>,
    {
        self.into().map(Into::into)
    }

    /// Maps the error of the result using `Into::into`.
    ///
    /// # Errors
    ///
    /// ... self explanatory
    fn err_into<U>(self) -> Result<T, U>
    where
        E: Into<U>,
    {
        self.into().map_err(Into::into)
    }
}

/// Extension methods for `Result<T, E>` to work with boxed errors.
#[autoimpl]
pub trait BoxErrExt<T, E: Into<BoxErr>>: Into<Result<T, E>> {
    /// Box the error type of the response.
    ///
    /// # Errors
    ///
    /// ... self explanatory.
    fn box_err(self) -> Result<T, BoxErr> {
        self.into().map_err(Into::into)
    }

    /// Box the error type of the response and then call `Into::into`.
    ///
    /// # Errors
    ///
    /// ... self explanatory.
    fn box_err_into<U>(self) -> Result<T, U>
    where
        BoxErr: Into<U>,
    {
        self.box_err().map_err(Into::into)
    }

    /// Box then map the error type of the response.
    ///
    /// # Errors
    ///
    /// ... self explanatory.
    fn box_map_err<U, F>(self, f: F) -> Result<T, U>
    where
        F: FnOnce(BoxErr) -> U,
    {
        self.box_err().map_err(f)
    }
}

/// Extension methods for `Future` to work with boxed errors.
#[autoimpl]
pub trait FutBoxErrExt: TryFuture + Sized {
    /// Box the error type of the future.
    fn box_err(self) -> BoxErrFut<Self> {
        BoxErrFut(self)
    }

    /// Box the error type of the future and then call `Into::into`.
    fn box_err_into<U>(self) -> BoxErrIntoFut<Self, U> {
        BoxErrIntoFut(self, PhantomData)
    }

    /// Box then map the error type of the future.
    fn box_map_err<U, F>(self, f: F) -> BoxMapErrFut<Self, F>
    where
        F: FnOnce(BoxErr) -> U,
    {
        BoxMapErrFut::Incomplete(self, f)
    }
}

/// A future that boxes its error.
#[pin_project]
#[derive(Debug)]
pub struct BoxErrFut<F>(#[pin] F);

impl<F: TryFuture> Future for BoxErrFut<F>
where
    F::Error: Into<BoxErr>,
{
    type Output = Result<F::Ok, BoxErr>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().0.try_poll(cx).map_err(Into::into)
    }
}

/// A future that boxes its error and then calls `Into::into`.
#[pin_project]
#[derive(Debug)]
pub struct BoxErrIntoFut<F, U>(#[pin] F, PhantomData<U>);

impl<F: TryFuture, U> Future for BoxErrIntoFut<F, U>
where
    F::Error: Into<BoxErr>,
    BoxErr: Into<U>,
{
    type Output = Result<F::Ok, U>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        (self.project().0)
            .try_poll(cx)
            .map_err(Into::into)
            .map_err(Into::into)
    }
}

/// A future that boxes its error and then maps it.
///
/// This future panics if polled after completion.
#[pin_project(project = BoxMapErrProj, project_replace = BoxMapErrRepl)]
#[derive(Debug)]
pub enum BoxMapErrFut<F, M> {
    /// The future is incomplete.
    Incomplete(#[pin] F, M),

    /// The future is complete.
    Complete,
}

impl<F: TryFuture, M, U> Future for BoxMapErrFut<F, M>
where
    F::Error: Into<BoxErr>,
    M: FnOnce(BoxErr) -> U,
{
    type Output = Result<F::Ok, U>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match self.as_mut().project() {
            BoxMapErrProj::Incomplete(f, _) => {
                let t = ready!(f.try_poll(cx));

                match self.project_replace(BoxMapErrFut::Complete) {
                    BoxMapErrRepl::Incomplete(_, m) => match t {
                        Ok(t) => Poll::Ready(Ok(t)),
                        Err(e) => Poll::Ready(Err(m(e.into()))),
                    },

                    BoxMapErrRepl::Complete => unreachable!(),
                }
            }

            BoxMapErrProj::Complete => panic!("polled after completion"),
        }
    }
}

/// Adds a `with` method to into-iterators.
#[autoimpl]
pub trait With: IntoIterator + FromIterator<Self::Item> + Sized {
    /// Adds an item to the end of the iterable object, returning itself.
    #[must_use]
    fn with(self, item: Self::Item) -> Self {
        self.into_iter().chain(Some(item)).collect()
    }

    /// Adds multiple items to the end of the iterable object, returning itself.
    #[must_use]
    fn with_many<I>(self, items: I) -> Self
    where
        I: IntoIterator<Item = Self::Item>,
    {
        self.into_iter().chain(items).collect()
    }
}

/// Enables racing futures to retrieve just the fastest one.
#[autoimpl]
#[async_trait]
pub trait TryRace<F>: IntoIterator<Item = F> + Sized
where
    F: TryFuture + Send,
{
    /// Race the futures to retrieve just the fastest one.
    async fn try_race(self) -> Result<F::Ok, F::Error> {
        let mut futs = Vec::new();

        for fut in self {
            futs.push(fut.into_future().boxed());
        }

        match futures::future::select_ok(futs).await {
            Ok((t, _)) => Ok(t),
            Err(err) => Err(err),
        }
    }
}
