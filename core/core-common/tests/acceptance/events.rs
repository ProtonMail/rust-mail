use std::sync::Arc;

use proton_core_api::services::proton::{Action, ContactEventV6, ContactId, ContactRootEventV6};
use proton_core_common::{
    event_loop::v6::{ContactEventCache, ContactEventV6Subscriber},
    test_utils::test_context::TestContext,
};
use proton_event_loop::v6::EventSubscriber;

#[tokio::test]
async fn deleted_contact_does_not_fail_event_poll() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let subscriber = ContactEventV6Subscriber::from(Arc::downgrade(&user_ctx));
    let mut cache = ContactEventCache::default();

    let contact_id = ContactId::from("my_id");

    let event = ContactEventV6 {
        id: contact_id.clone(),
        action: Action::Update,
    };

    let event = ContactRootEventV6 {
        contacts: Some(vec![event]),
        labels: None,
        refresh: false,
        has_more: false,
    };

    ctx.mock_get_full_contact_does_not_exist(contact_id).await;

    // Fire event:
    subscriber.on_event(&event, &mut cache).await.unwrap();
}
