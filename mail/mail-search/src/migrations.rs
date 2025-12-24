//! Database migrations for mail-search crate
//!
//! This module defines migrations for search-related database tables.
//! Migrations are applied via `mail-common` which calls `search_migrations()`.

use include_dir::{Dir, include_dir};
use proton_sqlite3::file::embedded_migrations;

/// Get search-related migrations
///
/// This function returns migrations for search index tables.
/// These migrations should be merged into the main migration sequence
/// in `mail-common/src/db/offline_migrations.rs`.
pub fn search_migrations() -> Vec<Box<dyn proton_sqlite3::Migration + 'static>> {
    const MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/migrations");
    embedded_migrations(&MIGRATIONS)
}
