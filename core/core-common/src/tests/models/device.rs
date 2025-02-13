use proton_core_test_utils::test_context::TestContext;

use crate::{datatypes::DeviceEnvironment, models::RegisteredDevice};

#[tokio::test]
async fn test_save_registered_device_and_retrieve_it() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let mut device_to_register = RegisteredDevice {
        device_token: "ABCD".to_string(),

        environment: DeviceEnvironment::Google,

        public_key: None,

        ping_notification_status: None,

        push_notification_status: None,

        local_id: None,
        row_id: None,
    };

    let mut tether = user_ctx.stash().connection();
    let tx = tether.transaction().await.unwrap();
    device_to_register.save(&tx).await.unwrap();

    tx.commit().await.unwrap();

    let cached_device = RegisteredDevice::get(&tether)
        .await
        .expect("Cached device")
        .expect("Cached device");

    assert_eq!(cached_device.device_token, device_to_register.device_token);
    assert_eq!(cached_device.environment, device_to_register.environment);
    assert_eq!(cached_device.public_key, device_to_register.public_key);
    assert_eq!(
        cached_device.ping_notification_status,
        device_to_register.ping_notification_status
    );
    assert_eq!(
        cached_device.push_notification_status,
        device_to_register.push_notification_status
    );
}

#[tokio::test]
async fn only_last_device_token_can_be_retrieved() {
    // Scenario: App crashes without the proper sign-off.

    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let mut tether = user_ctx.stash().connection();
    let tx = tether.transaction().await.unwrap();

    let mut first = RegisteredDevice {
        device_token: "ABCD".to_string(),

        environment: DeviceEnvironment::Google,

        public_key: None,

        ping_notification_status: None,

        push_notification_status: None,

        local_id: None,
        row_id: None,
    };

    first.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    // Crash
    //
    // ...
    //
    // Recovery
    let mut tether = user_ctx.stash().connection();
    let tx = tether.transaction().await.unwrap();

    let mut second = RegisteredDevice {
        device_token: "ABCD".to_string(),

        environment: DeviceEnvironment::Google,

        public_key: None,

        ping_notification_status: None,

        push_notification_status: None,

        local_id: None,
        row_id: None,
    };

    second.save(&tx).await.unwrap();

    tx.commit().await.unwrap();

    let cached_device = RegisteredDevice::get(&tether)
        .await
        .expect("Cached device")
        .expect("Cached device");

    assert_eq!(cached_device.device_token, second.device_token);
    assert_eq!(cached_device.environment, second.environment);
    assert_eq!(cached_device.public_key, second.public_key);
    assert_eq!(
        cached_device.ping_notification_status,
        second.ping_notification_status
    );
    assert_eq!(
        cached_device.push_notification_status,
        second.push_notification_status
    );
}
