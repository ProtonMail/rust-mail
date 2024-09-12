pub mod account;
mod core;

#[cfg(test)]
#[path = "../tests/db/migrations.rs"]
mod tests;

pub use account::migrate_account_db;
pub use core::migrate_core_db;
