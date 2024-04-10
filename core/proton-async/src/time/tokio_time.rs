//! Tokio based time implementations.
use std::time::Duration;

pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}
