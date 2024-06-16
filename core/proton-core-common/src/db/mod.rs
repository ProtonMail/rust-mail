//! Core related database for user sessions and user info.
//!
//! The module provide 2 distinct connection types which can be used interchangeably. It is up
//! to the user of this crate to decide whether they wish to store the user info in the same
//! or separate databases.

mod addresses;
mod contacts;
mod core;
mod migrations;
pub(crate) mod session;

pub use migrations::*;
pub use session::*;

pub use proton_sqlite3;
#[cfg(test)]
use stash::stash::Stash;

pub type DBResult<T> = proton_sqlite3::rusqlite::Result<T>;
pub type DBError = proton_sqlite3::rusqlite::Error;

#[cfg(test)]
async fn new_core_test_connection() -> Stash {
    use std::io::stdout;
    use tracing::subscriber::set_global_default;
    use tracing::Level;
    use tracing_subscriber::fmt::layer;
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{registry, EnvFilter};
    drop(set_global_default(
        registry()
            .with(EnvFilter::new(
                "debug,stash=debug",
            ))
            .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
    ));
    use crate::db::migrations::migrate_core_db;
    let stash = Stash::new(None).expect("Failed to create Stash");
    migrate_core_db(&stash).await.unwrap();
    stash
}

#[macro_export]
macro_rules! new_u64_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize,
        )]
        #[serde(crate = "self::serde")]
        #[repr(transparent)]
        pub struct $name(pub u64);

        impl $name {
            #[must_use]
            pub fn new(v: u64) -> Self {
                Self(v)
            }

            #[must_use]
            pub fn value(&self) -> u64 {
                self.0
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }

        #[allow(unused_qualifications)]
        impl proton_sqlite3::rusqlite::types::FromSql for $name {
            fn column_result(
                value: proton_sqlite3::rusqlite::types::ValueRef<'_>,
            ) -> proton_sqlite3::rusqlite::types::FromSqlResult<Self> {
                u64::column_result(value).map($name)
            }
        }

        #[allow(unused_qualifications)]
        impl proton_sqlite3::rusqlite::types::ToSql for $name {
            fn to_sql(
                &self,
            ) -> proton_sqlite3::rusqlite::Result<proton_sqlite3::rusqlite::types::ToSqlOutput<'_>>
            {
                self.0.to_sql()
            }
        }

        #[cfg(feature = "uniffi")]
        uniffi::custom_newtype!($name, u64);
    };
}

pub use new_u64_type;
