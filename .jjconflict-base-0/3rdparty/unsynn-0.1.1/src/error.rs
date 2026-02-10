use crate::{TokenIter, TokenStream};
use std::sync::Arc;

/// Result type for parsing.
pub type Result<T> = std::result::Result<T, Error>;

// To keep the Error passing simple and allocation free for the common cases we define these
// common cases plus adding the generic case as dyn boxed error.
/// Actual kind of an error.
#[derive(Clone)]
pub enum ErrorKind {
    /// A no error state that can be upgraded by later errors.
    NoError,
    /// Trying to parse `expected`.
    UnexpectedToken {
        /// type name of what was expected
        expected: &'static str,
        /// Iterator starting at the error
        at: <TokenStream as IntoIterator>::IntoIter,
    },
    /// Something else failed which can be fully formatted as `String`.
    Other {
        /// explanation what failed
        reason: String,
    },
    /// Any other error.
    Dynamic(Arc<dyn std::error::Error>),
}

/// Error type for parsing.
#[must_use]
#[derive(Clone)]
pub struct Error {
    /// Kind of the error.
    pub kind: ErrorKind,
    // ShadowCountedIter position where it happened
    // on disjunct parsers we use this to determine which error to keep
    pos: usize,
}

impl Error {
    /// Create a `ErrorKind::NoError` error.
    #[allow(clippy::missing_errors_doc)]
    pub const fn no_error() -> Self {
        Error {
            kind: ErrorKind::NoError,
            pos: 0,
        }
    }

    /// Upgrade an error to one with greater pos value.
    #[allow(clippy::missing_errors_doc)]
    pub fn upgrade<T>(&mut self, r: Result<T>) -> Result<T> {
        if let Err(other) = &r {
            if matches!(self.kind, ErrorKind::NoError) || other.pos > self.pos {
                *self = other.clone();
            }
        }
        r
    }

    /// Set the position of the error.
    ///
    /// Sometimes the position of the error is not known at the time of creation. This allows
    /// to adjust it later.
    pub fn set_pos(&mut self, pos: impl TokenCount) {
        self.pos = pos.token_count();
    }

    /// Get the position of the error.
    #[must_use]
    pub const fn pos(&self) -> usize {
        self.pos
    }

    /// Create a `Result<T>::Err(Error{ kind: ErrorKind::Unexpected })` error.
    #[allow(clippy::missing_errors_doc)]
    pub fn unexpected_token<T>(at: &TokenIter) -> Result<T> {
        Err(Error {
            kind: ErrorKind::UnexpectedToken {
                expected: std::any::type_name::<T>(),
                at: at.clone().into_inner_iter(),
            },
            pos: at.token_count(),
        })
    }

    /// Create a `Result<T>::Err(Error{ kind: ErrorKind::UnexpectedEnd })` error.
    #[allow(clippy::missing_errors_doc)]
    pub fn unexpected_end<T>() -> Result<T> {
        Err(Error {
            kind: ErrorKind::UnexpectedToken {
                expected: std::any::type_name::<T>(),
                at: TokenStream::new().into_iter(),
            },
            pos: usize::MAX,
        })
    }

    /// Create a `Result<T>::Err(Error{ kind: ErrorKind::Other })` error.
    #[allow(clippy::missing_errors_doc)]
    pub fn other<T>(pos: impl TokenCount, reason: String) -> Result<T> {
        Err(Error {
            kind: ErrorKind::Other { reason },
            pos: pos.token_count(),
        })
    }

    /// Create a `Error::Dynamic` error.
    pub fn dynamic(pos: impl TokenCount, err: impl std::error::Error + 'static) -> Self {
        Error {
            kind: ErrorKind::Dynamic(Arc::new(err)),
            pos: pos.token_count(),
        }
    }
}

impl std::error::Error for Error {}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.kind {
            ErrorKind::NoError => {
                write!(f, "NoError")
            }
            ErrorKind::UnexpectedToken { expected, at } => {
                write!(
                    f,
                    "Unexpected token: expected {expected}, found {at:?} at {:?}",
                    at.clone().next().map(|s| s.span().start())
                )
            }
            ErrorKind::Other { reason } => {
                write!(f, "{reason}")
            }
            ErrorKind::Dynamic(err) => {
                write!(f, "Error: {err}")
            }
        }
    }
}

impl std::fmt::Display for Error {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.kind {
            ErrorKind::NoError => {
                write!(f, "NoError")
            }
            ErrorKind::UnexpectedToken { expected, at } => {
                write!(
                    f,
                    "Unexpected token: expected {expected}, found {at:?} at {:?}",
                    at.clone().next().map(|s| s.span().start())
                )
            }
            ErrorKind::Other { reason } => {
                write!(f, "{reason}")
            }
            ErrorKind::Dynamic(err) => {
                write!(f, "Error: {err}")
            }
        }
    }
}

/// We track the position of the error by counting tokens. This trait is implemented for
/// references to shadow counted `TokenIter`, and `usize`. The later allows to pass in a
/// position directly or use `usize::MAX` in case no position data is available (which will
/// make this error the be the final one when upgrading).
pub trait TokenCount {
    /// Get the position of the token iterator.
    fn token_count(self) -> usize;
}

// Allows passing a usize directly.
impl TokenCount for usize {
    #[inline]
    fn token_count(self) -> usize {
        self
    }
}

impl TokenCount for &TokenIter<'_> {
    #[inline]
    fn token_count(self) -> usize {
        self.counter()
    }
}

impl TokenCount for &mut TokenIter<'_> {
    #[inline]
    fn token_count(self) -> usize {
        self.counter()
    }
}

// implementing for &&mut allows us to pass a &mut TokenIter by reference when it is still needed
// later. Otherwise it would need to be reborrow '&mut *iter' which is less ergonomic.
impl TokenCount for &&mut TokenIter<'_> {
    #[inline]
    fn token_count(self) -> usize {
        self.counter()
    }
}
