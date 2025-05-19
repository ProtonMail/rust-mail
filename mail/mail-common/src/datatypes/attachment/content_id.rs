use serde::{Deserialize, Serialize};
use stash::exports::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use uuid::Uuid;

/// Represents an attachment content id.
///
/// The content id for the attachment is only returned by API as a header wrapped with `<>`. This
/// id checks for this and removes it.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ContentId(String);

impl ContentId {
    /// Create a new random content id.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing value.
    ///
    /// If the value is wrapped in `<>` this will be stripped.
    pub fn with<T: AsRef<str>>(value: T) -> Self {
        let value = value.as_ref();
        if value.starts_with('<') && value.ends_with('>') {
            return Self(value[1..value.len() - 1].to_owned());
        }
        Self(value.to_owned())
    }

    pub fn into_inner(self) -> String {
        self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for ContentId {
    fn default() -> Self {
        Self::new()
    }
}

impl ToSql for ContentId {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

impl FromSql for ContentId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        String::column_result(value).map(Self)
    }
}

impl<T: AsRef<str>> From<T> for ContentId {
    fn from(value: T) -> Self {
        Self::with(value.as_ref())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn strips_prefix_and_suffix() {
        let input = "<foo>";
        let expected = "foo";
        let content_id = ContentId::with(input);
        assert_eq!(content_id.as_str(), expected);
    }

    #[test]
    fn does_not_strip_partial_prefix() {
        let input = "<foo";
        let expected = "<foo";
        let content_id = ContentId::with(input);
        assert_eq!(content_id.as_str(), expected);
    }

    #[test]
    fn does_not_strip_partial_suffix() {
        let input = "foo>";
        let expected = "foo>";
        let content_id = ContentId::with(input);
        assert_eq!(content_id.as_str(), expected);
    }
}
