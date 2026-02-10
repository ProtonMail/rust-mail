//! ## Error
//!
//! This module defines the primary error type(s) used throughout Muon.
// re-export app from the error module

use crate::common::BoxErr;
use derive_more::{Debug, Display};
use std::error::Error as StdError;
use std::fmt::{Formatter, Result as FmtResult};

/// A `Result` alias where the `Err` case is `muon::Error`.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// An error that may occur when using muon.
#[derive(Debug)]
pub struct Error {
    /// The kind of error.
    kind: ErrorKind,

    /// The source of the error, if any.
    src: Option<BoxErr>,

    /// Whether the error is retryable.
    retryable: bool,
}

impl Error {
    pub(crate) fn new<E: Into<BoxErr>>(kind: ErrorKind, src: Option<E>) -> Error {
        Error {
            kind,
            src: src.map(Into::into),
            retryable: false,
        }
    }

    pub(crate) fn map_kind(self, kind: ErrorKind) -> Error {
        Error { kind, ..self }
    }

    pub(crate) fn with_retryable(self, retryable: bool) -> Error {
        Error { retryable, ..self }
    }

    /// Get the kind of error.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Whether the error is retryable.
    pub fn retryable(&self) -> bool {
        self.retryable
    }

    /// Create an "other" error.
    ///
    /// This is useful when implementing layers outside this crate.
    pub fn other<E: Into<BoxErr>>(src: E) -> Error {
        Error::new(ErrorKind::Other, Some(src))
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        if let Some(src) = &self.src {
            Some(src.source().unwrap_or(src.as_ref()))
        } else {
            None
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        if let Some(src) = &self.src {
            write!(f, "{}: {src}", self.kind)
        } else {
            write!(f, "{}", self.kind)
        }
    }
}

/// The kinds of errors that can occur.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// Authentication failure.
    #[display("failed to authenticate")]
    Auth,

    /// TLS error.
    #[display("TLS error")]
    Tls,

    /// Host resolution failure.
    #[display("failed to resolve host")]
    Resolve,

    /// Host dialing failure.
    #[display("failed to dial host")]
    Dial,

    /// Host connection failure.
    #[display("failed to connect to host")]
    Connect,

    /// Request sending failure.
    #[display("failed to send request")]
    Send,

    /// The connection was closed.
    #[display("connection closed")]
    #[deprecated(note = "use `Send` and/or `Error::retryable` instead")]
    Closed,

    /// Error in request.
    #[display("error in request")]
    Req,

    /// Error in response.
    #[display("error in response")]
    Res,

    /// Other error.
    #[display("other error")]
    Other,
}

impl ErrorKind {
    pub(crate) fn auth<E: Into<BoxErr>>(src: E) -> Error {
        Error::new(ErrorKind::Auth, Some(src))
    }

    pub(crate) fn tls<E: Into<BoxErr>>(src: E) -> Error {
        Error::new(ErrorKind::Tls, Some(src))
    }

    pub(crate) fn resolve<E: Into<BoxErr>>(src: E) -> Error {
        Error::new(ErrorKind::Resolve, Some(src))
    }

    pub(crate) fn dial<E: Into<BoxErr>>(src: E) -> Error {
        Error::new(ErrorKind::Dial, Some(src))
    }

    pub(crate) fn connect<E: Into<BoxErr>>(src: E) -> Error {
        Error::new(ErrorKind::Connect, Some(src))
    }

    pub(crate) fn send<E: Into<BoxErr>>(src: E) -> Error {
        Error::new(ErrorKind::Send, Some(src))
    }

    pub(crate) fn req<E: Into<BoxErr>>(src: E) -> Error {
        Error::new(ErrorKind::Req, Some(src))
    }

    pub(crate) fn res<E: Into<BoxErr>>(src: E) -> Error {
        Error::new(ErrorKind::Res, Some(src))
    }
}
