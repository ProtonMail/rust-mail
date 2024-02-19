//! General purpose utilities.

/// Generate a unique type for a string based id.
#[macro_export]
macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Hash, Clone)]
        #[serde(crate = "self::serde")]
        /// Id for an API Event.
        pub struct $name(pub String);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl<T: Into<String>> From<T> for $name {
            fn from(v: T) -> Self {
                Self(v.into())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        #[cfg(feature = "uniffi")]
        uniffi::custom_newtype!($name, String);

        #[cfg(feature = "sql")]
        impl proton_sqlite3::rusqlite::ToSql for $name {
            fn to_sql(
                &self,
            ) -> proton_sqlite3::rusqlite::Result<proton_sqlite3::rusqlite::ToSqlOutput<'_>> {
                self.0.to_sql()
            }
        }

        #[cfg(feature = "sql")]
        impl proton_sqlite3::rusqlite::types::FromSql for $name {
            fn column_result(
                value: ValueRef<'_>,
            ) -> proton_sqlite3::rusqlite::types::FromSqlResult<Self> {
                String::column_result(value).map($name)
            }
        }
    };
}

pub use string_id;
