use proton_core_common::Context;
use proton_core_common::test_utils::test_context::TestContext;
use proton_core_common::{
    models::{AppProtection, AppSettings, ModelExtension, PinProtection},
    pin_code::{PinCode, PinError},
};
use stash::orm::Model;

#[tokio::test]
async fn create_and_delete_pin() {
    let test_ctx = TestContext::new().await;
    let core_ctx = test_ctx.core_context();
    let pin = vec![1, 2, 3, 4];

    PinCode::set(core_ctx.clone(), pin.clone()).await.unwrap();

    let tether = core_ctx.account_stash().connection().await.unwrap();
    let app_settings = AppSettings::get_or_default(&tether).await;

    assert_eq!(app_settings.protection, AppProtection::Pin);
    let mut pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 0);

    core_ctx.last_access_reset();

    let incorrect_pin = vec![0, 0, 0, 0];

    let error = PinCode::delete(core_ctx.clone(), incorrect_pin)
        .await
        .unwrap_err();

    assert!(matches!(error, PinError::IncorrectPin));

    pin_metadata.reload(&tether).await.unwrap();
    assert_eq!(pin_metadata.attempts, 1);

    core_ctx.last_access_reset();

    PinCode::delete(core_ctx.clone(), pin).await.unwrap();

    assert!(PinProtection::get(&tether).await.unwrap().is_none());
    assert_eq!(
        AppSettings::get_or_default(&tether).await.protection,
        AppProtection::None
    );
}

#[tokio::test]
async fn modify_pin() {
    let test_ctx = TestContext::new().await;
    let core_ctx = test_ctx.core_context();
    let pin = vec![1, 2, 3, 4];

    PinCode::set(core_ctx.clone(), pin.clone()).await.unwrap();

    let tether = core_ctx.account_stash().connection().await.unwrap();
    let app_settings = AppSettings::get_or_default(&tether).await;

    assert_eq!(app_settings.protection, AppProtection::Pin);
    let pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 0);

    core_ctx.last_access_reset();

    PinCode::verify(core_ctx.clone(), pin.clone())
        .await
        .unwrap();

    let new_pin = vec![0, 0, 0, 0];
    let old_pin = pin;

    // Lets create new pin
    PinCode::set(core_ctx.clone(), new_pin.clone())
        .await
        .unwrap();

    core_ctx.last_access_reset();

    let error = PinCode::verify(core_ctx.clone(), old_pin)
        .await
        .unwrap_err();
    assert!(matches!(error, PinError::IncorrectPin));

    core_ctx.last_access_reset();

    PinCode::verify(core_ctx.clone(), new_pin).await.unwrap();

    let count = PinProtection::count("", vec![], &tether).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn validation_max_attempts() {
    let test_ctx = TestContext::new().await;
    let core_ctx = test_ctx.core_context();
    let pin = vec![1, 2, 3, 4];
    let incorrect_pin = vec![0, 0, 0, 0];

    PinCode::set(core_ctx.clone(), pin).await.unwrap();

    let mut tether = core_ctx.account_stash().connection().await.unwrap();
    let app_settings = AppSettings::get_or_default(&tether).await;

    assert_eq!(app_settings.protection, AppProtection::Pin);
    let mut pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 0);

    core_ctx.last_access_reset();

    // Lets pretend we have one attempt left
    pin_metadata.attempts = PinCode::MAX_ATTEMPTS - 1;

    tether
        .tx(async |tx| pin_metadata.save(tx).await)
        .await
        .unwrap();

    let error = PinCode::verify(core_ctx.clone(), incorrect_pin.clone())
        .await
        .unwrap_err();

    assert!(matches!(error, PinError::TooManyAttempts));

    let pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 10);

    core_ctx.last_access_reset();

    // Pin code is not responsible to do anything regarding `TooManyAttempts`
    // error. In production flow there is catch on this error
    // which nukes databases and caches.
    let error = PinCode::verify(core_ctx.clone(), incorrect_pin)
        .await
        .unwrap_err();

    assert!(matches!(error, PinError::TooManyAttempts));

    let pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 11);
}

#[tokio::test]
async fn deleting_not_existing_pin_multiple_times_should_succeed() {
    let test_ctx = TestContext::new().await;
    let core_ctx = test_ctx.core_context();

    let mut tether = core_ctx.account_stash().connection().await.unwrap();
    let mut app_settings = AppSettings::get_or_default(&tether).await;
    app_settings.set_biometrics();
    tether
        .tx(async |tx| app_settings.save(tx).await)
        .await
        .unwrap();

    assert!(PinProtection::get(&tether).await.unwrap().is_none());
    assert_eq!(
        AppSettings::get_or_default(&tether).await.protection,
        AppProtection::Biometrics
    );

    assert!(PinCode::delete(core_ctx.clone(), vec![]).await.is_ok());
    assert!(PinCode::delete(core_ctx.clone(), vec![]).await.is_ok());
    assert!(PinCode::delete(core_ctx.clone(), vec![]).await.is_ok());

    assert!(PinProtection::get(&tether).await.unwrap().is_none());
    assert_eq!(
        AppSettings::get_or_default(&tether).await.protection,
        AppProtection::None
    );
}

trait PinContextExt {
    fn last_access_reset(&self);
}

impl PinContextExt for Context {
    fn last_access_reset(&self) {
        self.clock().pin_code_reset();
    }
}
