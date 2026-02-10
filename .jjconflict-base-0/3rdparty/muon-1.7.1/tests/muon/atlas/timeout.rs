use crate::atlas::new_client;
use anyhow::Result;
use muon::util::{DurationExt, ProtonRequestExt};
use muon::GET;

#[tokio::test]
async fn test_timeout_request_total() -> Result<()> {
    let c = new_client();

    // Set the total timeout of this request to 0 seconds: fail immediately.
    assert!(GET!("/tests/ping")
        .allowed_time(0.s())
        .send_with(&c)
        .await
        .is_err());

    // Set the total timeout of this request to 999 seconds: succeed.
    assert!(GET!("/tests/ping")
        .allowed_time(999.s())
        .send_with(&c)
        .await
        .is_ok());

    Ok(())
}
