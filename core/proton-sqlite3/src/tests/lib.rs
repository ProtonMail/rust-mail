#![allow(non_snake_case)]

use stash::stash::Stash;

#[tokio::test]
async fn test_nested_transactions_trigger_error() {
    let stash = Stash::new(None).expect("Failed to create Stash");
    let conn = stash.connection();
    conn.transaction()
        .await
        .expect("Failed to start transaction");
    conn.transaction()
        .await
        .expect_err("Nested transactions should trigger errors");
}
