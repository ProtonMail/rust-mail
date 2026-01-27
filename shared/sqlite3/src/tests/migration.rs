use super::*;
use file::embedded_migrations;
use include_dir::{Dir, include_dir};
use stash::{UserDb, stash::Stash};

#[tokio::test]
async fn test_migration() {
    const TABLE: &str = "test_table_version";

    let stash: Stash = Stash::new(None).expect("failed to create stash");
    let mut tether = stash.connection().await.unwrap();

    Migrator::new(TABLE, vec![Box::new(M1)])
        .migrate(&mut tether)
        .await
        .expect("Failed to run migration");

    let expected = Migrator::new(TABLE, vec![Box::new(M1), Box::new(M2)])
        .migrate(&mut tether)
        .await
        .expect("Failed to run migration");
    assert_eq!(expected, 2);

    let expected = Migrator::new(TABLE, vec![Box::new(M1), Box::new(M2)])
        .migrate(&mut tether)
        .await
        .expect("Failed to run migration");
    assert_eq!(expected, 2);
}

#[tokio::test]
async fn test_verification() {
    const TABLE: &str = "test_table_version";

    let stash: Stash = Stash::new(None).expect("failed to create stash");
    let mut tether = stash.connection().await.unwrap();

    // ---

    let migrator = Migrator::new(TABLE, vec![Box::new(M1)]);
    let actual = migrator.verify(&mut tether).await.unwrap_err();

    let expected = MigratorError::VersionMismatch {
        got: None,
        expected: 1,
    };

    assert_eq!(expected.to_string(), actual.to_string());

    migrator.migrate(&mut tether).await.unwrap();
    migrator.verify(&mut tether).await.unwrap();

    // ---

    let migrator = Migrator::new(TABLE, vec![Box::new(M1), Box::new(M2)]);
    let actual = migrator.verify(&mut tether).await.unwrap_err();

    let expected = MigratorError::VersionMismatch {
        got: Some(1),
        expected: 2,
    };

    assert_eq!(expected.to_string(), actual.to_string());

    migrator.migrate(&mut tether).await.unwrap();
    migrator.verify(&mut tether).await.unwrap();
}

#[tokio::test]
async fn test_migration_with_different_table_ids() {
    const TABLE_1: &str = "test_table_version_foo";
    const TABLE_2: &str = "test_table_version_bar";

    let stash: Stash = Stash::new(None).expect("failed to create stash");
    let mut tether = stash.connection().await.unwrap();

    Migrator::new(TABLE_1, vec![Box::new(M1)])
        .migrate(&mut tether)
        .await
        .expect("Failed to run migration");

    Migrator::new(TABLE_2, vec![Box::new(M2)])
        .migrate(&mut tether)
        .await
        .expect("Failed to run migration");
}

#[test]
fn test_migrations_ordering() {
    let mut migrations: Vec<Box<dyn Migration<UserDb>>> = vec![Box::new(M1), Box::new(M2)];
    sort_migrations_and_check_for_conflicts(&mut migrations);

    assert_eq!(migrations[0].name(), "002_m1");
    assert_eq!(migrations[1].name(), "003_m2");
}

#[tokio::test]
async fn test_file_migrations() {
    const TABLE: &str = "test_table_version";
    const MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/tests/migrations");

    let stash: Stash = Stash::new(None).expect("failed to create stash");
    let mut tether = stash.connection().await.unwrap();

    let version = Migrator::new(TABLE, embedded_migrations(&MIGRATIONS))
        .migrate(&mut tether)
        .await
        .expect("Failed to run migration");

    assert_eq!(version, 2);
}

#[tokio::test]
async fn test_mixing_code_and_file_migrations() {
    const TABLE: &str = "test_table_version";
    const MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/tests/migrations");

    let migrations = {
        let mut migrations = embedded_migrations(&MIGRATIONS);

        migrations.push(Box::new(M1));
        migrations
    };

    let stash: Stash = Stash::new(None).expect("failed to create stash");
    let mut tether = stash.connection().await.unwrap();

    let version = Migrator::new(TABLE, migrations)
        .migrate(&mut tether)
        .await
        .expect("Failed to run migration");

    assert_eq!(version, 3);
}

struct M1;

#[async_trait::async_trait]
impl Migration<UserDb> for M1 {
    fn name(&self) -> &'static str {
        "002_m1"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        tx.execute("CREATE TABLE test1 (ID INTEGER)", vec![])
            .await?;

        Ok(())
    }
}

struct M2;

#[async_trait::async_trait]
impl Migration<UserDb> for M2 {
    fn name(&self) -> &'static str {
        "003_m2"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        tx.execute("CREATE TABLE test2 (ID INTEGER)", vec![])
            .await?;

        Ok(())
    }
}
