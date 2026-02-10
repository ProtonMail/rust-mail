//! While at it's core, [`crate::Migration`] is just a trait, that allows us to execute
//! any rust code, in most cases we do not need embedding SQL in Rust. Instead we should be able to just run plain SQL migrations.
//!
//! This module provides [`EmbeddedFileMigration`] that does exactly that.
//!

use std::path::Path;

use include_dir::Dir;
use stash::marker::DatabaseMarker;
use stash::stash::Bond;

use crate::Migration;

/// Migration that loads embedded SQL file and executes statement by statement (separated by `;`).
/// In most cases that file migration was loaded from a file during `build.rs`.
///
pub struct EmbeddedFileMigration {
    /// Path of where the file existed.
    ///
    pub path: &'static Path,

    /// Textual representation of SQL statements, separated by `;`.
    ///
    pub migration_content: &'static str,
}

#[async_trait::async_trait]
impl<Db: DatabaseMarker> Migration<Db> for EmbeddedFileMigration {
    fn name(&self) -> &str {
        self.path.to_str().unwrap_or("000_unknown.sql")
    }

    async fn migrate(&self, tx: &Bond<'_, Db>) -> Result<(), stash::stash::StashError> {
        let statements = self.migration_content.trim();

        tx.batch(statements).await?;

        Ok(())
    }
}

/// Loads embedded migrations
///
#[must_use]
pub fn embedded_migrations<Db: DatabaseMarker>(dir: &Dir<'static>) -> Vec<Box<dyn Migration<Db>>> {
    dir.find("**/*.sql")
        .unwrap()
        .filter_map(|entry| entry.as_file())
        .map(|file| EmbeddedFileMigration {
            path: file.path(),
            migration_content: file.contents_utf8().unwrap(),
        })
        .map(|m| {
            let migration: Box<dyn Migration<Db>> = Box::new(m);
            migration
        })
        .collect::<Vec<_>>()
}
