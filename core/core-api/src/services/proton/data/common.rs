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
    pub fn new(email: String) -> Self {
        Self(email)
    }

    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn as_ref(&self) -> PrivateEmailRef<'_> {
        PrivateEmailRef(self.0.as_str())
    }
}

impl std::fmt::Display for PrivateEmail {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", sanitize(&self.0))
    }
}

impl std::fmt::Debug for PrivateEmail {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", sanitize(&self.0))
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

impl std::ops::Deref for PrivateEmail {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

#[cfg(feature = "sql")]
impl ::stash::exports::ToSql for PrivateEmail {
    fn to_sql(&self) -> Result<::stash::exports::ToSqlOutput<'_>, ::stash::exports::SqliteError> {
        self.as_str().to_sql()
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
    pub fn as_str(&self) -> &str {
        self.0
    }

    #[must_use]
    pub fn to_owned(&self) -> PrivateEmail {
        PrivateEmail(self.0.to_owned())
    }
}

impl std::fmt::Display for PrivateEmailRef<'_> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", sanitize(self.0))
    }
}

impl std::fmt::Debug for PrivateEmailRef<'_> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "PrivateEmailRef({})", sanitize(self.0))
    }
}

fn sanitize(value: &str) -> String {
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
