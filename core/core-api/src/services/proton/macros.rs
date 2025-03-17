/// Declare a new unique type for a Proton String Identifier.
///
/// # Example
///
/// ```
/// use proton_api_core::declare_proton_id;
/// declare_proton_id!(pub MyProtonId);
///
/// let id = MyProtonId::from("my-actual-proton-id");
/// ```
#[macro_export]
macro_rules! declare_proton_id {
    (
        $(#[$($attrss:tt)*])*
        $visibility:vis $name:ident
    ) => {
        $(#[$($attrss)*])*
        #[derive(Clone, Debug, serde::Deserialize, Eq, Hash, PartialEq, serde::Serialize)]
        $visibility struct $ name(String);

        impl $name {
            #[doc ="Create a new [`"]
            #[doc =stringify!($name)]
            #[doc ="`] from a [`String`]."]
            ///
            /// # Parameters
            ///
            /// * `id` - The ID to wrap.
            ///
            #[must_use]
            pub fn new(id: String) -> Self {
                Self(id)
            }

            #[doc = "Convert the [`"]
            #[doc = stringify!($name)]
            #[doc = "`] into the inner [`String`]."]
            #[must_use]
            pub fn into_inner(self) -> String {
                self.0
            }

            /// Get a reference to the inner [`String`]
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name{
            fn from(id: String) -> Self {
                Self(id)
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.to_owned())
            }
        }

        impl ::std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                self.0.as_str()
            }
        }

        #[cfg(feature = "sql")]
        impl ::stash::exports::ToSql for $name {
            fn to_sql(&self) -> Result<::stash::exports::ToSqlOutput<'_>, ::stash::exports::SqliteError> {
                self.as_str().to_sql()
            }
        }

        #[cfg(feature = "sql")]
        impl ::stash::exports::FromSql for $name {
            fn column_result(value: stash::exports::ValueRef<'_>) -> ::stash::exports::FromSqlResult<Self> {
                String::column_result(value).map(Self)
            }
        }

        impl $crate::services::proton::ProtonIdSqlMarker for $name {}

        impl $crate::services::proton::ProtonIdMarker for $name {}
    }
}
