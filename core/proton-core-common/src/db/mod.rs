//! Core related database for user sessions and user info.
//!
//! The module provide 2 distinct connection types which can be used interchangeably. It is up
//! to the user of this crate to decide whether they wish to store the user info in the same
//! or separate databases.
use proton_sqlite3::{new_connection_wrapper, new_tracked_connection_wrapper, MigratorError};
use std::ops::Deref;

mod addresses;
mod core;
mod migrations;
mod session;

pub use migrations::*;
pub use session::*;

pub use proton_sqlite3;

pub type DBResult<T> = proton_sqlite3::rusqlite::Result<T>;
pub type DBError = proton_sqlite3::rusqlite::Error;
pub type DBMigrationError = MigratorError;

new_tracked_connection_wrapper!(CoreSqliteConnection);
new_connection_wrapper!(SessionSqliteConnection);

#[cfg(test)]
fn new_core_test_connection() -> CoreSqliteConnection {
    use proton_sqlite3::{InProcessTrackerService, SqliteConnectionPool, SqliteMode};
    let pool = SqliteConnectionPool::new(SqliteMode::InMemory, false);
    {
        let mut conn = pool.acquire().unwrap();
        migrate_core_db(&mut conn).unwrap();
    }
    let tracker = InProcessTrackerService::new(pool).expect("failed to create tracker service");
    tracker
        .new_connection()
        .expect("failed to acquire connection")
        .into()
}
