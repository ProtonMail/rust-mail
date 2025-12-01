use std::fmt::{Debug, Display};

use url::Url;

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaybeUri {
    Uri(Url),
    Text(String),
}

impl<T: AsRef<str> + Into<String>> From<T> for MaybeUri {
    fn from(value: T) -> Self {
        match Url::parse(value.as_ref()) {
            Ok(uri) => Self::Uri(uri),
            _ => Self::Text(value.into()),
        }
    }
}

impl Default for MaybeUri {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl Display for MaybeUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaybeUri::Uri(url) => write!(f, "{url}"),
            MaybeUri::Text(t) => write!(f, "{t}"),
        }
    }
}

/// A Uri as defined in RFC3986
#[derive(Debug, Clone, PartialEq, Ord, PartialOrd, Eq)]
pub struct Uri(pub Url);

impl Uri {
    /// Create a new Uri from a URL
    #[must_use]
    pub fn new(value: Url) -> Self {
        Self(value)
    }

    /// Try to create a new URI from a str
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&str> for Uri {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        Ok(Self(Url::parse(value).map_err(|_| {
            VCardValueError::Invalid(ValueType::Uri, value.to_owned())
        })?))
    }
}
