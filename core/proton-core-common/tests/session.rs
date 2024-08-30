use common::TestContext;
use stash::orm::ResultsetChange;

mod common;

#[tokio::test]
async fn test_session_state() {
    let ctx = TestContext::new().await;
    let real_ctx = ctx.context();
    let user_ctx = ctx.user_context().await;

    // Initially, state should not exist.
    if let Ok(states) = real_ctx.get_session_states().await {
        assert!(states.is_empty());
    } else {
        panic!("failed to get session states");
    };

    // Also, user should return no state.
    let Ok(None) = user_ctx.state().await else {
        panic!("expected no state to exist");
    };

    // Mark the session as active.
    let Ok(()) = user_ctx.set_active().await else {
        panic!("failed to set active");
    };

    // Now, the state should exist.
    if let Ok(states) = real_ctx.get_session_states().await {
        assert_eq!(states.len(), 1);
    } else {
        panic!("failed to get session states");
    };

    // And the user should return the state with non-zero last active timestamp.
    if let Ok(Some(state)) = user_ctx.state().await {
        assert!(state.last_active_ts > 0);
    } else {
        panic!("expected state to exist");
    };

    // And the session should consider itself active.
    let Ok(true) = user_ctx.is_active().await else {
        panic!("expected session to be active");
    };
}

#[tokio::test]
async fn test_session_state_watcher() {
    let ctx = TestContext::new().await;
    let real_ctx = ctx.context();
    let user_ctx = ctx.user_context().await;

    // Watch the user's session state.
    let Ok((_, rx)) = real_ctx.watch_session_states().await else {
        panic!("failed to watch session states");
    };

    // Mark the session as active.
    let Ok(()) = user_ctx.set_active().await else {
        panic!("failed to set active");
    };

    // Expect to receive an event.
    if let Ok(ResultsetChange::Inserted(state)) = rx.recv_async().await {
        assert_eq!(&state.user_id, user_ctx.user_id());
    } else {
        panic!("expected event");
    };
}
