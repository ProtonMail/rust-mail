mod core;
mod session;

#[cfg(test)]
#[path = "../tests/db/migrations.rs"]
mod tests;

pub use core::migrate_core_db;
pub use session::migrate_session_db;
