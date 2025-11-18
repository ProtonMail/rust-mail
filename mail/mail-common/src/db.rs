pub mod migrations;

pub type DBMigrationError = MigratorError;

pub use proton_sqlite3;

use proton_sqlite3::migration::MigratorError;
