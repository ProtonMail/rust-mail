#[cfg(test)]
#[path = "tests/migration.rs"]
mod tests;

pub mod file;

#[allow(unused_imports)]
use futures::executor::block_on;
use itertools::Itertools;
use stash::exports::SqliteError;
use stash::marker::DatabaseMarker;
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use thiserror::Error;
use tracing::{Instrument, debug};

#[async_trait::async_trait]
pub trait Migration<Db: DatabaseMarker>: Send + Sync {
    fn name(&self) -> &str;

    fn order_number(&self) -> &str {
        let Some((order, _)) = self.name().split_once('_') else {
            panic!(
                "Expected migration name separated by `_`. Found `{}`",
                self.name()
            );
        };
        order
    }

    async fn migrate(&self, tx: &Bond<'_, Db>) -> Result<(), StashError>;
}

#[derive(Debug, Error)]
pub enum MigratorError {
    #[error("Found invalid version {0}")]
    InvalidVersion(usize),

    #[error("Version mismatch - got {got:?}, expected {expected}")]
    VersionMismatch { got: Option<usize>, expected: usize },

    #[error("Migration error: {0}")]
    Migration(#[from] rusqlite::Error),

    #[error("Stash error: {0}")]
    Stash(#[from] StashError),
}

pub struct Migrator<Db: DatabaseMarker> {
    table: String,
    migrations: Vec<Box<dyn Migration<Db>>>,
}

impl<Db: DatabaseMarker> Migrator<Db> {
    #[must_use]
    pub fn new(table: &str, mut migrations: Vec<Box<dyn Migration<Db>>>) -> Self {
        sort_migrations_and_check_for_conflicts(&mut migrations);

        Self {
            table: table.into(),
            migrations,
        }
    }

    /// Migrates database to the newest version.
    ///
    /// See: [`Self::verify()`].
    pub async fn migrate(&self, tether: &mut Tether<Db>) -> Result<usize, MigratorError> {
        let expected_version = get_expected_version(&self.migrations);
        let current_version = get_current_version(tether, &self.table).await?;
        if let Some(current_version) = current_version
            && current_version == expected_version
        {
            debug!("No migrations to run");
            return Ok(expected_version);
        }
        tether
            .tx(async |tx| {
                let current_version = if let Some(version) = current_version {
                    debug!("Found current version={version}");
                    if version > expected_version {
                        return Err(MigratorError::InvalidVersion(version));
                    }
                    version
                } else {
                    debug!("No version table found, initializing");
                    create_version_table(tx).await?;
                    set_current_version(tx, &self.table, 0).await?;
                    0
                };

                debug!(?current_version, ?expected_version, "Running migrations");
                run_migrations(tx, &self.table, current_version, &self.migrations).await?;
                debug!(?current_version, "Migrations complete");

                Ok(expected_version)
            })
            .await
    }

    /// Verifies that the database is exactly at the newest version - if that's
    /// not the case, returns an error.
    ///
    /// Note that this function does not run any migrations.
    ///
    /// See: [`Self::migrate()`].
    pub async fn verify(&self, tether: &mut Tether<Db>) -> Result<(), MigratorError> {
        tether
            .tx(async |tx| {
                let got = get_current_version(tx, &self.table).await?;
                let expected = get_expected_version(&self.migrations);

                if got == Some(expected) {
                    Ok(())
                } else {
                    Err(MigratorError::VersionMismatch { got, expected })
                }
            })
            .await
    }
}

fn get_expected_version<Db: DatabaseMarker>(m: &[Box<dyn Migration<Db>>]) -> usize {
    m.len()
}

async fn get_current_version<Db: DatabaseMarker>(
    tether: &Tether<Db>,
    table_name: &str,
) -> Result<Option<usize>, StashError> {
    let query = "SELECT COUNT(DISTINCT `name`) FROM sqlite_master WHERE `type`='table' AND name= ?";

    let count = tether
        .query_value::<_, u64>(query, params![VERSION_TABLE_NAME])
        .await?;

    if count == 0 {
        return Ok(None);
    }

    read_current_version(tether, table_name).await.map(Some)
}

const VERSION_TABLE_FIELD_ID: &str = "id";
const VERSION_TABLE_FIELD_VERSION: &str = "version";
const VERSION_TABLE_NAME: &str = "proton_sqlite3_db_version";

async fn read_current_version<Db: DatabaseMarker>(
    tether: &Tether<Db>,
    id: &str,
) -> Result<usize, StashError> {
    let query = format!(
        "SELECT {VERSION_TABLE_FIELD_VERSION} FROM {VERSION_TABLE_NAME} WHERE {VERSION_TABLE_FIELD_ID}=?"
    );
    let version = match tether
        .query_value::<_, u64>(query, params![id.to_owned()])
        .await
    {
        Ok(v) => v,
        Err(e) => {
            if matches!(
                e,
                StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
            ) {
                0
            } else {
                return Err(e);
            }
        }
    };
    #[allow(clippy::cast_possible_truncation)]
    Ok(version as usize)
}

async fn create_version_table<Db: DatabaseMarker>(tx: &Bond<'_, Db>) -> Result<(), StashError> {
    let query = format!(
        "CREATE TABLE {VERSION_TABLE_NAME} ({VERSION_TABLE_FIELD_ID} TEXT UNIQUE NOT NULL PRIMARY KEY, \
{VERSION_TABLE_FIELD_VERSION} INTEGER NOT NULL)"
    );
    tx.execute(query, vec![]).await?;
    Ok(())
}

async fn set_current_version<Db: DatabaseMarker>(
    tx: &Bond<'_, Db>,
    id: &str,
    version: usize,
) -> Result<(), StashError> {
    let query = format!(
        "INSERT INTO {VERSION_TABLE_NAME} ({VERSION_TABLE_FIELD_ID}, {VERSION_TABLE_FIELD_VERSION}) VALUES (?,?) \
ON CONFLICT({VERSION_TABLE_FIELD_ID}) DO UPDATE SET {VERSION_TABLE_FIELD_VERSION}=excluded.{VERSION_TABLE_FIELD_VERSION}"
    );
    tx.execute(query, params![id.to_owned(), version]).await?;
    Ok(())
}

async fn run_migrations<Db: DatabaseMarker>(
    tx: &Bond<'_, Db>,
    table_name: &str,
    current_version: usize,
    migrations: &[Box<dyn Migration<Db>>],
) -> Result<(), StashError> {
    for (i, m) in migrations.iter().enumerate().skip(current_version) {
        let span = tracing::debug_span!("migration", version = i, name = m.name());
        async move {
            debug!("Starting migration");
            m.migrate(tx).await?;
            debug!("Migration complete");
            let next_version = i + 1;
            set_current_version(tx, table_name, next_version).await?;
            debug!("Version updated to {next_version}");
            Ok::<_, StashError>(())
        }
        .instrument(span)
        .await?;
    }

    Ok(())
}

/// Since migrations might be implemented by many developers in parallel, it is crucial to ensure, that the ordering of those migrations is stable
/// and predictable.
///
/// We are using `0001_migration_name.sql` scheme, where a string before `_` is the order number.
///
/// This function sorts by the order number and panics, if there are `0001_a.sql` and `0001_b.sql`. Such a conflict indicates, that the
/// ordering is undecidable and it's developer's responsibility to rename one of the files.
///
fn sort_migrations_and_check_for_conflicts<Db: DatabaseMarker>(
    migrations: &mut [Box<dyn Migration<Db>>],
) {
    migrations.sort_by_key(|m| m.order_number().to_string());

    for (a, b) in migrations.iter().tuple_windows() {
        assert_ne!(
            a.order_number(),
            b.order_number(),
            "Two migrations share the same order number: `{}` and `{}`. Please resolve the conflict by renaming one of them.",
            a.name(),
            b.name()
        );
    }
}
