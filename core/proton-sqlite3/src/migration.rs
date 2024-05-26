//! Utilities to facility migration of the database.

use tracing::debug;
#[allow(unused_imports)]
use futures::executor::block_on;
use stash::params;
use stash::stash::{Stash, StashError, Tether};
use stash::datatypes::QueryResultU64;

/// Migration Unit.
pub trait Migration {
    /// Migration name.
    fn name(&self) -> &str;

    /// Perform the migration of from the previous version to the current version.
    ///
    /// # Params
    /// * `tx`: transaction on which to run the migration.
    ///
    /// # Errors
    ///
    /// Returns error if the migration failed to run.
    fn migrate(&self, tx: &Tether) -> Result<(), StashError>;
}

/// Possible errors that may occur during a migration.
#[derive(Debug, thiserror::Error)]
pub enum MigratorError {
    /// Database has an invalid version.
    #[error("Found invalid version {0}")]
    InvalidVersion(usize),
    /// Migration step failed.
    #[error("Migration error: {0}")]
    Migration(#[from] rusqlite::Error),
    #[error("Stash error: {0}")]
    Stash(#[from] StashError),
}

/// Utility to class to migrate sqlite database between version. See [`Migrator::migrate`] for more
/// info.
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
        stash: &Stash,
        version_table_name: &str,
        migrations: &[Box<dyn Migration>],
    ) -> Result<usize, MigratorError> {
        let tx = stash.transaction().await?;
            let expected_version = version_from_migration_list(migrations);
            // Check if version table exists, if not we are at version 0.
            let current_version =
                if let Some(version) = get_current_table_version(&tx, version_table_name).await? {
                    debug!("Found current version={version}");
                    if version > expected_version {
                        return Err(MigratorError::InvalidVersion(version));
                    }
                    version
                } else {
                    debug!("No version table found, initializing");
                    create_version_table(&tx).await?;
                    set_version_table_version(&tx, version_table_name, 0).await?;
                    0
                };

            debug!("Running migrations");
            run_migrations(&tx, version_table_name, current_version, migrations).await?;
            debug!("Migrations complete");
            let version = version_from_migration_list(migrations);
        tx.commit().await?;
        Ok(version)
    }
}

fn version_from_migration_list(m: &[Box<dyn Migration>]) -> usize {
    m.len()
}
async fn get_current_table_version(
    tx: &Tether,
    table_name: &str,
) -> Result<Option<usize>, StashError> {
    let query = "SELECT COUNT(DISTINCT `name`) AS value FROM sqlite_master WHERE `type`='table' AND name= ?";
    let count = tx.query::<_, QueryResultU64>(query, params![VERSION_TABLE_NAME]).await?.first().unwrap().value;
    if count == 0 {
        return Ok(None);
    }

    read_current_table_version(tx, table_name).await.map(Some)
}

const VERSION_TABLE_FIELD_ID: &str = "id";
const VERSION_TABLE_FIELD_VERSION: &str = "version";

const VERSION_TABLE_NAME: &str = "proton_sqlite3_db_version";

async fn read_current_table_version(tx: &Tether, id: &str) -> Result<usize, StashError> {
    let query = format!(
        "SELECT {VERSION_TABLE_FIELD_VERSION} AS value FROM {VERSION_TABLE_NAME} WHERE {VERSION_TABLE_FIELD_ID}=?"
    );
    let version = tx.query::<_, QueryResultU64>(query, params![id.to_owned()]).await?.first().map_or(0, |record| record.value);
    #[allow(clippy::cast_possible_truncation)]
    Ok(version as usize)
}

async fn create_version_table(tx: &Tether) -> Result<(), StashError> {
    let query = format!(
        "CREATE TABLE {VERSION_TABLE_NAME} ({VERSION_TABLE_FIELD_ID} TEXT UNIQUE NOT NULL PRIMARY KEY, \
{VERSION_TABLE_FIELD_VERSION} INTEGER NOT NULL)"
    );
    tx.execute(query, vec![]).await?;
    Ok(())
}

async fn set_version_table_version(
    tx: &Tether,
    id: &str,
    version: usize,
) -> Result<(), StashError> {
    let query = format!("INSERT INTO {VERSION_TABLE_NAME} ({VERSION_TABLE_FIELD_ID}, {VERSION_TABLE_FIELD_VERSION}) VALUES (?,?) \
ON CONFLICT({VERSION_TABLE_FIELD_ID}) DO UPDATE SET {VERSION_TABLE_FIELD_VERSION}=excluded.{VERSION_TABLE_FIELD_VERSION}");
    tx.execute(query, params![id.to_owned(), version]).await?;
    Ok(())
}

async fn run_migrations(
    tx: &Tether,
    table_name: &str,
    current_version: usize,
    migrations: &[Box<dyn Migration>],
) -> Result<(), StashError> {
    for (i, m) in migrations.iter().enumerate().skip(current_version) {
        let span = tracing::debug_span!("migration", version = i, name = m.name());
        {
            let _entered = span.enter();
            debug!("Starting migration");
            m.migrate(tx)?;
            debug!("Migration complete");
            let next_version = i + 1;
            set_version_table_version(tx, table_name, next_version).await?;
            debug!("Version updated to {next_version}");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_migration() {
    const TEST_TABLE_NAME: &str = "test_table_version";

    let stash = Stash::new(None).expect("failed to create stash");
    let migrator = Migrator::new();

    // first version
    let version = migrator
        .migrate(&stash, TEST_TABLE_NAME, &[create_migration_1()]).await
        .expect("Failed to run migration");
    assert_eq!(version, 1);

    // second version
    let version = migrator
        .migrate(
            &stash,
            TEST_TABLE_NAME,
            &[create_migration_1(), create_migration_2()],
        ).await
        .expect("Failed to run migration");
    assert_eq!(version, 2);

    // fail on downgrade
    let err = migrator
        .migrate(&stash, TEST_TABLE_NAME, &[create_migration_1()]).await
        .expect_err("Migration should fail");

    assert!(matches!(err, MigratorError::InvalidVersion(2)))
}

#[tokio::test]
async fn test_migration_with_different_table_ids() {
    const TEST_TABLE_NAME_1: &str = "test_table_version_foo";
    const TEST_TABLE_NAME_2: &str = "test_table_version_bar";
    
    let stash = Stash::new(None).expect("failed to create stash");
    let migrator = Migrator::new();

    // first version
    let version = migrator
        .migrate(&stash, TEST_TABLE_NAME_1, &[create_migration_1()]).await
        .expect("Failed to run migration");
    assert_eq!(version, 1);

    // second version
    let version = migrator
        .migrate(&stash, TEST_TABLE_NAME_2, &[create_migration_2()]).await
        .expect("Failed to run migration");
    assert_eq!(version, 1);
}

#[cfg(test)]
fn create_migration_1() -> Box<dyn Migration> {
    struct M {}

    impl Migration for M {
        fn name(&self) -> &str {
            "m1"
        }
        fn migrate(&self, tx: &Tether) -> Result<(), StashError> {
            block_on(async {
            tx.execute("CREATE TABLE test1 (ID INTEGER)", vec![]).await
            })?;
            Ok(())
        }
    }

    Box::new(M {})
}
#[cfg(test)]
fn create_migration_2() -> Box<dyn Migration> {
    struct M {}

    impl Migration for M {
        fn name(&self) -> &str {
            "m2"
        }
        fn migrate(&self, tx: &Tether) -> Result<(), StashError> {
            block_on(async {
            tx.execute("CREATE TABLE test2 (ID INTEGER)", vec![]).await
            })?;
            Ok(())
        }
    }

    Box::new(M {})
}
