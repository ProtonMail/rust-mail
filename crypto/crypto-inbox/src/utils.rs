/// Generate a unique type for a string based API identifier.
#[macro_export]
macro_rules! string_id {
    (
        $(#[$meta:meta])*
        $name:ident
    ) => {
        #[derive(Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq, Hash, Clone)]
        $(#[$meta])*
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

        impl ::std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                self.0.as_str()
            }
        }
    };
}
