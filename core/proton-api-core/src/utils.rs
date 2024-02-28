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
        impl $crate::exports::proton_sqlite3::rusqlite::types::ToSql for $name {
            fn to_sql(
                &self,
            ) -> $crate::exports::proton_sqlite3::rusqlite::Result<
                $crate::exports::proton_sqlite3::rusqlite::types::ToSqlOutput<'_>,
            > {
                self.0.to_sql()
            }
        }

        #[cfg(feature = "sql")]
        impl $crate::exports::proton_sqlite3::rusqlite::types::FromSql for $name {
            fn column_result(
                value: $crate::exports::proton_sqlite3::rusqlite::types::ValueRef<'_>,
            ) -> $crate::exports::proton_sqlite3::rusqlite::types::FromSqlResult<Self> {
                String::column_result(value).map($name)
            }
        }
    };
}

pub use string_id;

/// Generate all the boilerplate for an enum that is backed by a certain an integer representation.
/// ```
/// use proton_api_core::new_integer_enum;
/// use proton_api_core::exports::serde_repr;
/// new_integer_enum!(u8, Foo {
///     Bar = 0,
///     Foo = 1,
/// });
/// new_integer_enum!(u32, Bar{
///     Bar = 0,
///     Foo = 1,
/// });
/// ```
#[macro_export]
macro_rules! new_integer_enum {
    ($repr:tt, $name:ident {$($enum:ident=$value:tt,)+}) => {
        #[derive(Debug, Copy, Clone, $crate::exports::serde_repr::Serialize_repr, $crate::exports::serde_repr::Deserialize_repr, Eq, PartialEq, Hash)]
        #[serde(crate = "self::serde")]
        #[repr($repr)]
        #[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
        pub enum $name {
            $($enum = $value),+
        }

        #[cfg(feature = "sql")]
        impl proton_sqlite3::rusqlite::types::ToSql for $name {
            fn to_sql(
                &self,
            ) -> proton_sqlite3::rusqlite::Result<proton_sqlite3::rusqlite::types::ToSqlOutput<'_>> {
                Ok(proton_sqlite3::rusqlite::types::ToSqlOutput::Owned(
                    proton_sqlite3::rusqlite::types::Value::Integer(*self as i64),
                ))
            }
        }

        #[cfg(feature = "sql")]
        impl proton_sqlite3::rusqlite::types::FromSql for $name {
            fn column_result(
                value: proton_sqlite3::rusqlite::types::ValueRef<'_>,
            ) -> proton_sqlite3::rusqlite::types::FromSqlResult<Self> {
                match i64::column_result(value)? {
                    $($value => Ok($name::$enum),)+
                    v => Err(proton_sqlite3::rusqlite::types::FromSqlError::OutOfRange(v)),
                }
            }
        }
    };
}

pub use new_integer_enum;
