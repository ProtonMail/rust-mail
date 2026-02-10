use crate::db::migrations::{migrate_account_db, migrate_core_db};
use stash::stash::Stash;

#[tokio::test]
async fn test_core_migration_on_empty_data_set() {
    let stash = Stash::new(None).expect("Failed to create Stash");
    migrate_core_db(&stash).await.expect("failed to migrate");
}

#[tokio::test]
async fn test_session_migration_on_empty_data_set() {
    let stash = Stash::new(None).expect("Failed to create Stash");
    migrate_account_db(&stash).await.expect("failed to migrate");
}
