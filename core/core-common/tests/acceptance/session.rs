use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_common::OnSessionDeletedResponse;
use proton_core_common::db::account::CoreSession;
use proton_core_common::models::ModelExtension;
use proton_core_common::services::SessionObserverService;
use proton_core_common::test_utils::test_context::TestContext;
use stash::stash::{Bond, StashError};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
#[allow(unused_variables)]
async fn test_session_state() {
    let ctx = TestContext::new().await;
    let real_ctx = ctx.context();
    let user_ctx = ctx.user_context().await;
}

#[tokio::test]
#[allow(unused_variables)]
async fn test_session_state_watcher() {
    let ctx = TestContext::new().await;
    let real_ctx = ctx.context();
    let user_ctx = ctx.user_context().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_session_delete_subscriber() {
    let ctx = TestContext::new().await;
    let real_ctx = ctx.context();
    let user_ctx = ctx.user_context().await;

    let session_id = user_ctx.session_id().clone();
    let user_id = user_ctx.user_id().clone();
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<()>(1);
    let event_service = ctx.context().event_service();
    let session_observer_service = ctx.context.get_service::<SessionObserverService>();
    session_observer_service.on_session_deleted(
        event_service,
        move |deleted_session_id: SessionId, deleted_user_id: UserId| {
            let deleted_session_id = deleted_session_id.clone();
            let deleted_user_id = deleted_user_id.clone();
            let sender = sender.clone();
            let user_id = user_id.clone();
            let session_id = session_id.clone();
            async move {
                assert_eq!(deleted_user_id, user_id);
                assert_eq!(deleted_session_id, session_id);
                sender.send(()).await.unwrap();
                OnSessionDeletedResponse::Terminate
            }
        },
    );

    real_ctx
        .account_stash()
        .connection()
        .await
        .unwrap()
        .tx(async |tx: &Bond<'_>| {
            assert_eq!(CoreSession::all(tx).await.unwrap().len(), 1);
            assert!(
                CoreSession::delete_by_id(user_ctx.session_id().clone(), tx)
                    .await
                    .unwrap(),
            );
            Ok::<_, StashError>(())
        })
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn test_force_logout_account_locally_deletes_sessions() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let user_id = user_ctx.user_id().clone();

    let tether = ctx.context().account_stash().connection().await.unwrap();
    assert_eq!(
        CoreSession::find_by_user_id(user_id.clone(), &tether)
            .await
            .unwrap()
            .len(),
        1
    );

    ctx.context()
        .force_logout_account_locally(user_id.clone())
        .await
        .unwrap();

    let tether = ctx.context().account_stash().connection().await.unwrap();
    assert_eq!(
        CoreSession::find_by_user_id(user_id.clone(), &tether)
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn test_event_poll_forbidden_forces_local_logout() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let user_id = user_ctx.user_id().clone();

    // Simulate production failure:
    // Forbidden: 403 Forbidden. Some(ApiErrorInfo { code: 9106, error: "Access token does not have sufficient scope", details: {"MissingScopes":["user"]} })
    Mock::given(method("GET"))
        .and(path("/api/core/v6/events/latest"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "Code": 9106,
            "Error": "Access token does not have sufficient scope",
            "Details": { "MissingScopes": ["user"] }
        })))
        .mount(ctx.mock_server())
        .await;

    let _ = user_ctx.poll_event_loop_impl().await;

    // Regardless of the poll result, we must have forced a local logout,
    // meaning no sessions remain for the account.
    let tether = ctx.context().account_stash().connection().await.unwrap();
    assert_eq!(
        CoreSession::find_by_user_id(user_id.clone(), &tether)
            .await
            .unwrap()
            .len(),
        0
    );
}
