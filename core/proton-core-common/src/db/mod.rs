//! Core related database for user sessions and user info.
//!
//! The module provide 2 distinct connection types which can be used interchangeably. It is up
//! to the user of this crate to decide whether they wish to store the user info in the same
//! or separate databases.

mod addresses;
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
    use crate::db::migrations::migrate_core_db;
    let stash = Stash::new(None).expect("Failed to create Stash");
    migrate_core_db(&stash).await.unwrap();
    stash
}
