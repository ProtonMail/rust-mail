use std::{borrow::Borrow, fmt, ops::Deref};

/// New type wrapper which hides all but the first character of the email address from Display and
/// Debug formatters to preserve user privacy.
///
/// This type is mainly intended to be used with any email address that is not a proton identity
/// associated with this account.
///
/// # Example
///
/// `foo@bar.com` is printed as `fXX@XX.XXX`
#[derive(
    Default, Clone, serde::Deserialize, Eq, Hash, PartialEq, Ord, PartialOrd, serde::Serialize,
)]
pub struct PrivateEmail(String);

impl PrivateEmail {
    #[must_use]
    pub fn new(email: impl Into<String>) -> Self {
        Self(email.into())
    }

    #[must_use]
    pub fn into_clear_text_string(self) -> String {
        self.0
    }

    #[must_use]
    pub fn as_clear_text_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn as_ref(&self) -> PrivateEmailRef<'_> {
        PrivateEmailRef(self.0.as_str())
    }
}

impl fmt::Display for PrivateEmail {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", sanitize_email(&self.0))
    }
}

impl fmt::Debug for PrivateEmail {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", sanitize_email(&self.0))
    }
}

impl From<String> for PrivateEmail {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for PrivateEmail {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl Borrow<str> for PrivateEmail {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl Deref for PrivateEmail {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

#[cfg(feature = "sql")]
impl ::stash::exports::ToSql for PrivateEmail {
    fn to_sql(&self) -> Result<::stash::exports::ToSqlOutput<'_>, ::stash::exports::SqliteError> {
        self.as_clear_text_str().to_sql()
    }
}

#[cfg(feature = "sql")]
impl ::stash::exports::FromSql for PrivateEmail {
    fn column_result(value: stash::exports::ValueRef<'_>) -> ::stash::exports::FromSqlResult<Self> {
        String::column_result(value).map(Self)
    }
}

#[derive(Clone, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct PrivateEmailRef<'s>(&'s str);

impl<'s> PrivateEmailRef<'s> {
    #[must_use]
    pub fn new(email: &'s str) -> PrivateEmailRef<'s> {
        Self(email)
    }

    #[must_use]
    pub fn as_clear_text_str(&self) -> &str {
        self.0
    }

    #[must_use]
    pub fn to_owned(&self) -> PrivateEmail {
        PrivateEmail(self.0.to_owned())
    }
}

impl fmt::Display for PrivateEmailRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", sanitize_email(self.0))
    }
}

impl fmt::Debug for PrivateEmailRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", sanitize_email(self.0))
    }
}

impl<'s, T: AsRef<str>> From<&'s T> for PrivateEmailRef<'s> {
    fn from(value: &'s T) -> Self {
        Self(value.as_ref())
    }
}

impl<'s> From<&'s str> for PrivateEmailRef<'s> {
    fn from(value: &'s str) -> Self {
        Self(value)
    }
}

fn sanitize_email(value: &str) -> String {
    value
        .chars()
        .enumerate()
        .map(|(index, c)| {
            if index == 0 {
                c
            } else if c != '@' {
                'X'
            } else {
                c
            }
        })
        .collect()
}

fn sanitize_string(value: &str) -> String {
    std::iter::repeat_n('x', value.chars().count()).collect()
}

/// New type wrapper which hides the string from Display and Debug outputs.
#[derive(
    Default, Clone, serde::Deserialize, Eq, Hash, PartialEq, Ord, PartialOrd, serde::Serialize,
)]
pub struct PrivateString(String);

impl PrivateString {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn into_clear_text_string(self) -> String {
        self.0
    }

    #[must_use]
    pub fn as_clear_text_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrivateString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", sanitize_string(&self.0))
    }
}

impl fmt::Debug for PrivateString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", sanitize_string(&self.0))
    }
}

impl From<String> for PrivateString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for PrivateString {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl Deref for PrivateString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

#[cfg(feature = "sql")]
impl ::stash::exports::ToSql for PrivateString {
    fn to_sql(&self) -> Result<::stash::exports::ToSqlOutput<'_>, ::stash::exports::SqliteError> {
        self.as_clear_text_str().to_sql()
    }
}

#[cfg(feature = "sql")]
impl ::stash::exports::FromSql for PrivateString {
    fn column_result(value: stash::exports::ValueRef<'_>) -> ::stash::exports::FromSqlResult<Self> {
        String::column_result(value).map(Self)
    }
}
