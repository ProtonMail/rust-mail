#![allow(non_snake_case)]

use super::*;

#[tokio::test]
async fn test_migration() {
    const TEST_TABLE_NAME: &str = "test_table_version";

    let stash = Stash::new(None).expect("failed to create stash");
    let migrator = Migrator::new();

    // first version
    let _version = migrator
        .migrate(&stash, TEST_TABLE_NAME, &[M1 {}])
        .await
        .expect("Failed to run migration");

    // second version
    let _version = migrator
        .migrate(&stash, TEST_TABLE_NAME, &[M2 {}])
        .await
        .expect("Failed to run migration");
}

#[tokio::test]
async fn test_migration_with_different_table_ids() {
    const TEST_TABLE_NAME_1: &str = "test_table_version_foo";
    const TEST_TABLE_NAME_2: &str = "test_table_version_bar";

    let stash = Stash::new(None).expect("failed to create stash");
    let migrator = Migrator::new();

    // first version
    let _version = migrator
        .migrate(&stash, TEST_TABLE_NAME_1, &[M1 {}])
        .await
        .expect("Failed to run migration");

    // second version
    let _version = migrator
        .migrate(&stash, TEST_TABLE_NAME_2, &[M2 {}])
        .await
        .expect("Failed to run migration");
}

struct M1 {}

impl Migration for M1 {
    fn name(&self) -> &'static str {
        "m1"
    }
    async fn migrate(&self, tx: &Bond) -> Result<(), StashError> {
        block_on(async { tx.execute("CREATE TABLE test1 (ID INTEGER)", vec![]).await })?;
        Ok(())
    }
}

struct M2 {}

impl Migration for M2 {
    fn name(&self) -> &'static str {
        "m2"
    }
    async fn migrate(&self, tx: &Bond) -> Result<(), StashError> {
        block_on(async { tx.execute("CREATE TABLE test2 (ID INTEGER)", vec![]).await })?;
        Ok(())
    }
}
