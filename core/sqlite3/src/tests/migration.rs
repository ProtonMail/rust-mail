#![allow(non_snake_case)]

use file::embedded_migrations;
use include_dir::{Dir, include_dir};
use stash::stash::Stash;

use super::*;

#[tokio::test]
async fn test_migration() {
    const TEST_TABLE_NAME: &str = "test_table_version";

    let stash = Stash::new(None).expect("failed to create stash");
    let mut tether = stash.connection();
    let migrator = Migrator::new();

    // first version
    let _version = migrator
        .migrate(&mut tether, TEST_TABLE_NAME, &mut [Box::new(M1)])
        .await
        .expect("Failed to run migration");

    // second version
    let _version = migrator
        .migrate(&mut tether, TEST_TABLE_NAME, &mut [Box::new(M2)])
        .await
        .expect("Failed to run migration");
}

#[tokio::test]
async fn test_migration_with_different_table_ids() {
    const TEST_TABLE_NAME_1: &str = "test_table_version_foo";
    const TEST_TABLE_NAME_2: &str = "test_table_version_bar";

    let stash = Stash::new(None).expect("failed to create stash");
    let mut tether = stash.connection();
    let migrator = Migrator::new();

    // first version
    let _version = migrator
        .migrate(&mut tether, TEST_TABLE_NAME_1, &mut [Box::new(M1)])
        .await
        .expect("Failed to run migration");

    // second version
    let _version = migrator
        .migrate(&mut tether, TEST_TABLE_NAME_2, &mut [Box::new(M2)])
        .await
        .expect("Failed to run migration");
}

#[test]
fn test_migrations_ordering() {
    let mut migrations: Vec<Box<dyn Migration>> = vec![Box::new(M1), Box::new(M2)];
    sort_migrations_and_check_for_conflicts(&mut migrations);

    assert_eq!(migrations[0].name(), "002_m1");
    assert_eq!(migrations[1].name(), "003_m2");
}

#[tokio::test]
async fn test_file_migrations() {
    static DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/tests/migrations");
    const TEST_TABLE_NAME: &str = "test_table_version";
    let mut migrations = embedded_migrations(&DIR);

    let stash = Stash::new(None).expect("failed to create stash");
    let mut tether = stash.connection();
    let migrator = Migrator::new();

    let version = migrator
        .migrate(&mut tether, TEST_TABLE_NAME, &mut migrations)
        .await
        .expect("Failed to run migration");

    assert_eq!(version, 2);
}

#[tokio::test]
async fn test_mixing_code_and_file_migrations() {
    static DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/tests/migrations");
    const TEST_TABLE_NAME: &str = "test_table_version";
    let mut migrations = embedded_migrations(&DIR);
    migrations.push(Box::new(M1));

    let stash = Stash::new(None).expect("failed to create stash");
    let mut tether = stash.connection();
    let migrator = Migrator::new();

    let version = migrator
        .migrate(&mut tether, TEST_TABLE_NAME, &mut migrations)
        .await
        .expect("Failed to run migration");

    assert_eq!(version, 3);
}

struct M1;

#[async_trait::async_trait]
impl Migration for M1 {
    fn name(&self) -> &'static str {
        "002_m1"
    }
    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        block_on(async { tx.execute("CREATE TABLE test1 (ID INTEGER)", vec![]).await })?;
        Ok(())
    }
}

struct M2;

#[async_trait::async_trait]
impl Migration for M2 {
    fn name(&self) -> &'static str {
        "003_m2"
    }
    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        block_on(async { tx.execute("CREATE TABLE test2 (ID INTEGER)", vec![]).await })?;
        Ok(())
    }
}
