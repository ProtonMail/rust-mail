#![allow(non_snake_case)]

use super::*;

#[tokio::test]
async fn test_migration() {
    const TEST_TABLE_NAME: &str = "test_table_version";

    let stash = Stash::new(None).expect("failed to create stash");
    let migrator = Migrator::new();

    // first version
    let version = migrator
        .migrate(&stash, TEST_TABLE_NAME, &[create_migration_1()])
        .await
        .expect("Failed to run migration");
    assert_eq!(version, 1);

    // second version
    let version = migrator
        .migrate(
            &stash,
            TEST_TABLE_NAME,
            &[create_migration_1(), create_migration_2()],
        )
        .await
        .expect("Failed to run migration");
    assert_eq!(version, 2);

    // fail on downgrade
    let err = migrator
        .migrate(&stash, TEST_TABLE_NAME, &[create_migration_1()])
        .await
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
        .migrate(&stash, TEST_TABLE_NAME_1, &[create_migration_1()])
        .await
        .expect("Failed to run migration");
    assert_eq!(version, 1);

    // second version
    let version = migrator
        .migrate(&stash, TEST_TABLE_NAME_2, &[create_migration_2()])
        .await
        .expect("Failed to run migration");
    assert_eq!(version, 1);
}

fn create_migration_1() -> Box<dyn Migration> {
    struct M {}

    impl Migration for M {
        fn name(&self) -> &str {
            "m1"
        }
        fn migrate(&self, tx: &Tether) -> Result<(), StashError> {
            block_on(async { tx.execute("CREATE TABLE test1 (ID INTEGER)", vec![]).await })?;
            Ok(())
        }
    }

    Box::new(M {})
}
fn create_migration_2() -> Box<dyn Migration> {
    struct M {}

    impl Migration for M {
        fn name(&self) -> &str {
            "m2"
        }
        fn migrate(&self, tx: &Tether) -> Result<(), StashError> {
            block_on(async { tx.execute("CREATE TABLE test2 (ID INTEGER)", vec![]).await })?;
            Ok(())
        }
    }

    Box::new(M {})
}
