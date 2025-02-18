use proton_core_test_utils::test_context::TestContext;
use proton_sqlite3::rusqlite::ErrorCode;
use stash::{orm::Model, stash::StashError};

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

// # Context
//
// There is a constraint, that only one row in `registered_devices` might exist.
// It is guarded by DB trigger.
//
// Additionally, [`RegisteredDevice::save`] prevents it by overwriting the last row.
//
// # What we test
//
// Since `::save` ensures there is only one row, we never test whether the database trigger is
// correct or not. Therefore let's make a scenario where developer
// by accident used `Model::save` instead.
#[tokio::test]
async fn should_trigger_db_guard_if_incorrectly_used_trait_method() {
    {
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

            row_id: None,
        };

        Model::save(&mut first, &tx).await.unwrap();
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

            row_id: None,
        };

        let stash_error = Model::save(&mut second, &tx).await.unwrap_err();
        tx.rollback().await.expect("Rolled back");

        let StashError::DeserializationError(stash::orm::ConversionError::SqliteError(
            sqlite_error,
        )) = stash_error
        else {
            panic!("Expected Sqlite Error, found {stash_error:?}")
        };
        assert_eq!(
            "registered_devices may have only one row. This is a bug in a model layer",
            sqlite_error.to_string()
        );
        let error = sqlite_error.sqlite_error().expect("Error");
        assert_eq!(error.code, ErrorCode::ConstraintViolation);
    }
}
