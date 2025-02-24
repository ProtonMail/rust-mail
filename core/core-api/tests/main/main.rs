use anyhow::Result;
use muon::test::server::Server;
use proton_api_core::human_verification::ChallengeObserver;
use proton_api_core::services::proton::ProtonCore;
use proton_api_core::session::{CoreSession, Session};
use proton_api_core::status_observer::StatusObserver;
use std::sync::Arc;

#[muon::test]
fn test_human_verification(s: Arc<Server>) -> Result<()> {
    tracing_subscriber::fmt::init();

    let session = Session::builder()
        .with_custom_env(s.env())
        .with_status(StatusObserver::test())
        .with_challenge(ChallengeObserver::new())
        .build()?;

    session.api().get_tests_ping(None, None).await?;

    Ok(())
}
