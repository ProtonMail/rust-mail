//! DEPRECATED
//!

mod migration;
mod tracker;

pub use migration::*;

// re-export;
pub use rusqlite;

#[tokio::test]
async fn test_nested_transactions_trigger_error() {
    let stash = stash::stash::Stash::new(None).expect("Failed to create Stash");
    let conn = stash.connection();
    conn.transaction()
        .await
        .expect("Failed to start transaction");
    conn.transaction()
        .await
        .expect_err("Nested transactions should trigger errors");
}
