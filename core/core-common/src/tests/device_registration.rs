use std::sync::{Arc, Mutex};

use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_common::test_utils::test_context::TestContext;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

// To break cyclic dependency
use proton_core_common::{
    datatypes::{DeviceEnvironment, RegisteredDevice},
    device_registration::{RegisteredDeviceTaskState, registered_device_task_step},
};
use wiremock::{
    Mock, Request, ResponseTemplate,
    matchers::{body_partial_json, method, path},
};

#[tokio::test]
async fn initial_registration() {
    let ctx = TestContext::new().await;

    let mut background_task_state = RegisteredDeviceTaskState::default();
    let (_sessions_tx, sessions_rx) = flume::bounded::<()>(16);
    let (device_tx, mut device_rx) = watch::channel::<Option<RegisteredDevice>>(None);
    let mut sessions_stream = sessions_rx.into_stream();

    let device_to_register = RegisteredDevice {
        device_token: "ABCD".to_string(),
        environment: DeviceEnvironment::Google,
        ping_notification_status: None,
        push_notification_status: None,
    };

    device_tx
        .send(Some(device_to_register))
        .expect("Could not send device to register");

    mock_ping_success(&ctx).await;
    let used_device_key = Arc::new(Mutex::new(None));
    mock_device_registration(
        &ctx,
        PartialTestRegisterDeviceRequest {
            device_token: "ABCD".to_string(),
            environment: proton_core_api::services::proton::DeviceEnvironment::Google,
            ping_notification_status: None,
            push_notification_status: None,
        },
        &used_device_key,
        1,
    )
    .await;

    let core_ctx = ctx.context();

    registered_device_task_step(
        core_ctx,
        &mut background_task_state,
        &mut sessions_stream,
        &mut device_rx,
    )
    .await
    .expect("Step");

    // It generated device key automatically
    assert!(used_device_key.lock().unwrap().is_some());
}

#[tokio::test]
async fn initial_registration_when_device_key_already_exist_in_keychain() {
    let ctx = TestContext::new().await;

    let mut background_task_state = RegisteredDeviceTaskState::default();
    let (_sessions_tx, sessions_rx) = flume::bounded::<()>(16);
    let (device_tx, mut device_rx) = watch::channel::<Option<RegisteredDevice>>(None);
    let mut sessions_stream = sessions_rx.into_stream();

    let device_to_register = RegisteredDevice {
        device_token: "ABCD".to_string(),
        environment: DeviceEnvironment::Google,
        ping_notification_status: None,
        push_notification_status: None,
    };

    device_tx
        .send(Some(device_to_register))
        .expect("Could not send device to register");

    mock_ping_success(&ctx).await;
    let used_device_key = Arc::new(Mutex::new(None));
    mock_device_registration(
        &ctx,
        PartialTestRegisterDeviceRequest {
            device_token: "ABCD".to_string(),
            environment: proton_core_api::services::proton::DeviceEnvironment::Google,
            ping_notification_status: None,
            push_notification_status: None,
        },
        &used_device_key,
        1,
    )
    .await;

    let core_ctx = ctx.context();

    // Imagine a scenario where keychain already has device key.
    // We want to reuse such a key.
    let pgp = proton_crypto::new_pgp_provider();

    let stored_public_key = core_ctx
        // It stores private key in keychain.
        // Later, we use private key to derive public key
        .gen_device_key_pair(&pgp)
        .unwrap()
        .to_string();

    registered_device_task_step(
        core_ctx,
        &mut background_task_state,
        &mut sessions_stream,
        &mut device_rx,
    )
    .await
    .expect("Step");

    // It used previously generated key
    assert_eq!(*used_device_key.lock().unwrap(), Some(stored_public_key));
}

#[tokio::test]
async fn test_device_token_changed() {
    let ctx = TestContext::new().await;

    let mut background_task_state = RegisteredDeviceTaskState::default();
    let (_sessions_tx, sessions_rx) = flume::bounded::<()>(16);
    let (device_tx, mut device_rx) = watch::channel::<Option<RegisteredDevice>>(None);
    let mut sessions_stream = sessions_rx.into_stream();

    let device_to_register = RegisteredDevice {
        device_token: "ABCD".to_string(),
        environment: DeviceEnvironment::Google,
        ping_notification_status: None,
        push_notification_status: None,
    };

    device_tx
        .send(Some(device_to_register))
        .expect("Could not send device to register");

    mock_ping_success(&ctx).await;
    let used_device_key = Arc::new(Mutex::new(None));
    mock_device_registration(
        &ctx,
        PartialTestRegisterDeviceRequest {
            device_token: "ABCD".to_string(),
            environment: proton_core_api::services::proton::DeviceEnvironment::Google,
            ping_notification_status: None,
            push_notification_status: None,
        },
        &used_device_key,
        1,
    )
    .await;

    let core_ctx = ctx.context();

    // Initial registration
    registered_device_task_step(
        core_ctx,
        &mut background_task_state,
        &mut sessions_stream,
        &mut device_rx,
    )
    .await
    .expect("Step");

    let device_to_register = RegisteredDevice {
        device_token: "EFGH".to_string(),
        environment: DeviceEnvironment::Google,
        ping_notification_status: None,
        push_notification_status: None,
    };

    device_tx
        .send(Some(device_to_register))
        .expect("Could not send device to register");

    ctx.mock_server().reset().await;

    mock_device_registration(
        &ctx,
        PartialTestRegisterDeviceRequest {
            device_token: "EFGH".to_string(),
            environment: proton_core_api::services::proton::DeviceEnvironment::Google,
            ping_notification_status: None,
            push_notification_status: None,
        },
        &used_device_key,
        1,
    )
    .await;

    // Registration token changed
    registered_device_task_step(
        core_ctx,
        &mut background_task_state,
        &mut sessions_stream,
        &mut device_rx,
    )
    .await
    .expect("Step");
}

#[tokio::test]
async fn register_more_than_one_session() {
    let ctx = TestContext::new().await;

    ctx.new_account(UserId::from("1234"), SessionId::from("TEST_UID_2"), None)
        .await;

    let mut background_task_state = RegisteredDeviceTaskState::default();
    let (_sessions_tx, sessions_rx) = flume::bounded::<()>(16);
    let (device_tx, mut device_rx) = watch::channel::<Option<RegisteredDevice>>(None);
    let mut sessions_stream = sessions_rx.into_stream();

    let device_to_register = RegisteredDevice {
        device_token: "ABCD".to_string(),
        environment: DeviceEnvironment::Google,
        ping_notification_status: None,
        push_notification_status: None,
    };

    device_tx
        .send(Some(device_to_register))
        .expect("Could not send device to register");

    mock_ping_success(&ctx).await;
    let used_device_key = Arc::new(Mutex::new(None));
    mock_device_registration(
        &ctx,
        PartialTestRegisterDeviceRequest {
            device_token: "ABCD".to_string(),
            environment: proton_core_api::services::proton::DeviceEnvironment::Google,
            ping_notification_status: None,
            push_notification_status: None,
        },
        &used_device_key,
        2,
    )
    .await;

    let core_ctx = ctx.context();

    registered_device_task_step(
        core_ctx,
        &mut background_task_state,
        &mut sessions_stream,
        &mut device_rx,
    )
    .await
    .expect("Step");

    // It generated device key automatically
    assert!(used_device_key.lock().unwrap().is_some());
}

#[tokio::test]
async fn register_new_session() {
    let ctx = TestContext::new().await;

    let mut background_task_state = RegisteredDeviceTaskState::default();
    let (sessions_tx, sessions_rx) = flume::bounded::<()>(16);
    let (device_tx, mut device_rx) = watch::channel::<Option<RegisteredDevice>>(None);
    let mut sessions_stream = sessions_rx.into_stream();

    let device_to_register = RegisteredDevice {
        device_token: "ABCD".to_string(),
        environment: DeviceEnvironment::Google,
        ping_notification_status: None,
        push_notification_status: None,
    };

    device_tx
        .send(Some(device_to_register))
        .expect("Could not send device to register");

    mock_ping_success(&ctx).await;
    let used_device_key = Arc::new(Mutex::new(None));
    mock_device_registration(
        &ctx,
        PartialTestRegisterDeviceRequest {
            device_token: "ABCD".to_string(),
            environment: proton_core_api::services::proton::DeviceEnvironment::Google,
            ping_notification_status: None,
            push_notification_status: None,
        },
        &used_device_key,
        1,
    )
    .await;

    let core_ctx = ctx.context();

    registered_device_task_step(
        core_ctx,
        &mut background_task_state,
        &mut sessions_stream,
        &mut device_rx,
    )
    .await
    .expect("Step");

    // It generated device key automatically
    assert!(used_device_key.lock().unwrap().is_some());

    ctx.new_account(UserId::from("1234"), SessionId::from("TEST_UID_2"), None)
        .await;
    sessions_tx.send(()).expect("Notify that session was added");

    ctx.mock_server().reset().await;
    mock_device_registration(
        &ctx,
        PartialTestRegisterDeviceRequest {
            device_token: "ABCD".to_string(),
            environment: proton_core_api::services::proton::DeviceEnvironment::Google,
            ping_notification_status: None,
            push_notification_status: None,
        },
        &used_device_key,
        1,
    )
    .await;

    registered_device_task_step(
        core_ctx,
        &mut background_task_state,
        &mut sessions_stream,
        &mut device_rx,
    )
    .await
    .expect("Step");
}

async fn mock_ping_success(ctx: &TestContext) {
    Mock::given(method("GET"))
        .and(path("/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(200))
        .mount(ctx.mock_server())
        .await;
}

async fn mock_device_registration(
    ctx: &TestContext,
    params: PartialTestRegisterDeviceRequest,
    key: &Arc<Mutex<Option<String>>>,
    times: u64,
) {
    let key = key.clone();

    Mock::given(method("POST"))
        .and(path("/api/core/v4/devices"))
        .and(body_partial_json(params))
        .respond_with(move |request: &Request| {
            let json = request
                .body_json::<RequestWithDeviceKey>()
                .expect("Could not deserialize");
            *key.lock().unwrap() = json.public_key;

            ResponseTemplate::new(200)
        })
        .expect(times)
        .mount(ctx.mock_server())
        .await;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RequestWithDeviceKey {
    /// PGP Public Key
    pub public_key: Option<String>,
}

/// Represents `POST /devices` request body.
///
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PartialTestRegisterDeviceRequest {
    /// Device token
    pub device_token: String,
    /// Environment to which we register
    pub environment: proton_core_api::services::proton::DeviceEnvironment,
    /// TODO: Document this field
    pub ping_notification_status: Option<i32>,
    /// TODO: Document this field
    pub push_notification_status: Option<i32>,
}
