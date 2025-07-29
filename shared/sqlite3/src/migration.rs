#[cfg(test)]
#[path = "tests/migration.rs"]
mod tests;

pub mod file;

#[allow(unused_imports)]
use futures::executor::block_on;
use itertools::Itertools;
use stash::exports::SqliteError;
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use thiserror::Error;
use tracing::{Instrument, debug};

#[async_trait::async_trait]
pub trait Migration: Send + Sync + 'static {
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

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError>;
}

#[derive(Debug, Error)]
pub enum MigratorError {
    #[error("Found invalid version {0}")]
    InvalidVersion(usize),
    #[error("Migration error: {0}")]
    Migration(#[from] rusqlite::Error),
    #[error("Stash error: {0}")]
    Stash(#[from] StashError),
}

#[derive(Default)]
pub struct Migrator {}

impl Migrator {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// In order to migrate a set of table, the migrator requires a table name where it will record
    /// the current version. If this table does not exist, it is assumed that the database is empty
    /// and needs to be initialized. If this table exists, the migrations are run until the version
    /// reaches the latest version.
    ///
    /// Migration version is determined by the number of migrations present in the `migrations`
    /// parameter. E.g.:
    /// ```rust,ignore
    ///  let migrations  = [
    ///     Migration_v1,
    ///     Migration_v2,
    ///     ...
    ///     Migration_vN
    ///   ];
    /// ```
    ///
    /// # Parameters
    ///
    /// * `conn`: sqlite connection for the migration
    /// * `version_table_name`: unique name under which to identify the version data
    /// * `migrations`: list of migrations to run
    ///
    /// # Errors
    ///
    /// Return error if the migration fails.
    ///
    pub async fn migrate(
        &self,
        tether: &mut Tether,
        version_table_name: &str,
        migrations: &mut [Box<dyn Migration>],
    ) -> Result<usize, MigratorError> {
        sort_migrations_and_check_for_conflicts(migrations);
        tether
            .tx(async |tx| {
                let expected_version = version_from_migration_list(migrations);
                // Check if version table exists, if not we are at version 0.
                let current_version = if let Some(version) =
                    get_current_table_version(tx, version_table_name).await?
                {
                    debug!("Found current version={version}");
                    if version > expected_version {
                        return Err(MigratorError::InvalidVersion(version));
                    }
                    version
                } else {
                    debug!("No version table found, initializing");
                    create_version_table(tx).await?;
                    set_version_table_version(tx, version_table_name, 0).await?;
                    0
                };

                debug!("Running migrations");
                run_migrations(tx, version_table_name, current_version, migrations).await?;
                debug!("Migrations complete");
                let version = version_from_migration_list(migrations);
                Ok(version)
            })
            .await
    }
}

fn version_from_migration_list(m: &[Box<dyn Migration>]) -> usize {
    m.len()
}
async fn get_current_table_version(
    tx: &Bond<'_>,
    table_name: &str,
) -> Result<Option<usize>, StashError> {
    let query = "SELECT COUNT(DISTINCT `name`) AS value FROM sqlite_master WHERE `type`='table' AND name= ?";
    let count = tx
        .query_value::<_, u64>(query, params![VERSION_TABLE_NAME])
        .await?;
    if count == 0 {
        return Ok(None);
    }

    read_current_table_version(tx, table_name).await.map(Some)
}

const VERSION_TABLE_FIELD_ID: &str = "id";
const VERSION_TABLE_FIELD_VERSION: &str = "version";

const VERSION_TABLE_NAME: &str = "proton_sqlite3_db_version";

async fn read_current_table_version(tx: &Bond<'_>, id: &str) -> Result<usize, StashError> {
    let query = format!(
        "SELECT {VERSION_TABLE_FIELD_VERSION} AS value FROM {VERSION_TABLE_NAME} WHERE {VERSION_TABLE_FIELD_ID}=?"
    );
    let version = match tx
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

async fn create_version_table(tx: &Bond<'_>) -> Result<(), StashError> {
    let query = format!(
        "CREATE TABLE {VERSION_TABLE_NAME} ({VERSION_TABLE_FIELD_ID} TEXT UNIQUE NOT NULL PRIMARY KEY, \
{VERSION_TABLE_FIELD_VERSION} INTEGER NOT NULL)"
    );
    tx.execute(query, vec![]).await?;
    Ok(())
}

async fn set_version_table_version(
    tx: &Bond<'_>,
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

async fn run_migrations(
    tx: &Bond<'_>,
    table_name: &str,
    current_version: usize,
    migrations: &[Box<dyn Migration>],
) -> Result<(), StashError> {
    for (i, m) in migrations.iter().enumerate().skip(current_version) {
        let span = tracing::debug_span!("migration", version = i, name = m.name());
        async move {
            debug!("Starting migration");
            m.migrate(tx).await?;
            debug!("Migration complete");
            let next_version = i + 1;
            set_version_table_version(tx, table_name, next_version).await?;
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
pub fn sort_migrations_and_check_for_conflicts(migrations: &mut [Box<dyn Migration>]) {
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
