//! Utilities to facility migration of the database.

use crate::{SqliteConnection, SqliteTransaction};
use rusqlite::OptionalExtension;
use tracing::debug;

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
    fn migrate(&self, tx: &mut SqliteTransaction) -> rusqlite::Result<()>;
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
    pub fn migrate(
        &self,
        conn: &mut SqliteConnection,
        version_table_name: &str,
        migrations: &[Box<dyn Migration>],
    ) -> Result<usize, MigratorError> {
        conn.tx(|tx| {
            let expected_version = version_from_migration_list(migrations);
            // Check if version table exists, if not we are at version 0.
            let current_version =
                if let Some(version) = get_current_table_version(tx, version_table_name)? {
                    debug!("Found current version={version}");
                    if version > expected_version {
                        return Err(MigratorError::InvalidVersion(version));
                    }
                    version
                } else {
                    debug!("No version table found, initializing");
                    create_version_table(tx)?;
                    set_version_table_version(tx, version_table_name, 0)?;
                    0
                };

            debug!("Running migrations");
            run_migrations(tx, version_table_name, current_version, migrations)?;
            debug!("Migrations complete");
            Ok(version_from_migration_list(migrations))
        })
    }
}

fn version_from_migration_list(m: &[Box<dyn Migration>]) -> usize {
    m.len()
}
fn get_current_table_version(
    tx: &mut SqliteTransaction,
    table_name: &str,
) -> rusqlite::Result<Option<usize>> {
    let query = "SELECT COUNT(DISTINCT `name`) FROM sqlite_master WHERE `type`='table' AND name= ?";
    let count = tx.query_row(query, [VERSION_TABLE_NAME], |r| r.get::<usize, u32>(0))?;
    if count == 0 {
        return Ok(None);
    }

    read_current_table_version(tx, table_name).map(Some)
}

const VERSION_TABLE_FIELD_ID: &str = "id";
const VERSION_TABLE_FIELD_VERSION: &str = "version";

const VERSION_TABLE_NAME: &str = "proton_sqlite3_db_version";

fn read_current_table_version(tx: &mut SqliteTransaction, id: &str) -> rusqlite::Result<usize> {
    let query = format!(
        "SELECT {VERSION_TABLE_FIELD_VERSION} FROM {VERSION_TABLE_NAME} WHERE {VERSION_TABLE_FIELD_ID}=?"
    );
    let version = tx.query_row(query, [id], |r| r.get(0)).optional()?;
    Ok(version.unwrap_or(0))
}

fn create_version_table(tx: &mut SqliteTransaction) -> rusqlite::Result<()> {
    let query = format!(
        "CREATE TABLE {VERSION_TABLE_NAME} ({VERSION_TABLE_FIELD_ID} TEXT UNIQUE NOT NULL PRIMARY KEY, \
{VERSION_TABLE_FIELD_VERSION} INTEGER NOT NULL)"
    );
    tx.execute(query, ())?;
    Ok(())
}

fn set_version_table_version(
    tx: &mut SqliteTransaction,
    id: &str,
    version: usize,
) -> rusqlite::Result<()> {
    let query = format!("INSERT INTO {VERSION_TABLE_NAME} ({VERSION_TABLE_FIELD_ID}, {VERSION_TABLE_FIELD_VERSION}) VALUES (?,?) \
ON CONFLICT({VERSION_TABLE_FIELD_ID}) DO UPDATE SET {VERSION_TABLE_FIELD_VERSION}=excluded.{VERSION_TABLE_FIELD_VERSION}");
    tx.execute(query, (id, version))?;
    Ok(())
}

fn run_migrations(
    tx: &mut SqliteTransaction,
    table_name: &str,
    current_version: usize,
    migrations: &[Box<dyn Migration>],
) -> rusqlite::Result<()> {
    for (i, m) in migrations.iter().enumerate().skip(current_version) {
        let span = tracing::debug_span!("migration", version = i, name = m.name());
        span.in_scope(|| -> rusqlite::Result<()> {
            debug!("Starting migration");
            m.migrate(tx)?;
            debug!("Migration complete");
            let next_version = i + 1;
            set_version_table_version(tx, table_name, next_version)?;
            debug!("Version updated to {next_version}");
            Ok(())
        })?;
    }

    Ok(())
}

#[test]
fn test_migration() {
    const TEST_TABLE_NAME: &str = "test_table_version";

    let pool = crate::SqliteConnectionPool::new(crate::SqliteMode::InMemory, true);
    let mut conn = pool.acquire().expect("failed to acquire connection");

    let migrator = Migrator::new();

    // first version
    let version = migrator
        .migrate(&mut conn, TEST_TABLE_NAME, &[create_migration_1()])
        .expect("Failed to run migration");
    assert_eq!(version, 1);

    // second version
    let version = migrator
        .migrate(
            &mut conn,
            TEST_TABLE_NAME,
            &[create_migration_1(), create_migration_2()],
        )
        .expect("Failed to run migration");
    assert_eq!(version, 2);

    // fail on downgrade
    let err = migrator
        .migrate(&mut conn, TEST_TABLE_NAME, &[create_migration_1()])
        .expect_err("Migration should fail");

    assert!(matches!(err, MigratorError::InvalidVersion(2)))
}

#[test]
fn test_migration_with_different_table_ids() {
    const TEST_TABLE_NAME_1: &str = "test_table_version_foo";
    const TEST_TABLE_NAME_2: &str = "test_table_version_bar";

    let pool = crate::SqliteConnectionPool::new(crate::SqliteMode::InMemory, true);
    let mut conn = pool.acquire().expect("failed to acquire connection");

    let migrator = Migrator::new();

    // first version
    let version = migrator
        .migrate(&mut conn, TEST_TABLE_NAME_1, &[create_migration_1()])
        .expect("Failed to run migration");
    assert_eq!(version, 1);

    // second version
    let version = migrator
        .migrate(&mut conn, TEST_TABLE_NAME_2, &[create_migration_2()])
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
        fn migrate(&self, tx: &mut SqliteTransaction) -> rusqlite::Result<()> {
            tx.execute("CREATE TABLE test1 (ID INTEGER)", ())?;
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
        fn migrate(&self, tx: &mut SqliteTransaction) -> rusqlite::Result<()> {
            tx.execute("CREATE TABLE test2 (ID INTEGER)", ())?;
            Ok(())
        }
    }

    Box::new(M {})
}
