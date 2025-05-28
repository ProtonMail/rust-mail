use proton_core_common::test_utils::test_context::TestContext;
use proton_core_common::{
    models::{AppProtection, AppSettings, ModelExtension, PinProtection},
    pin_code::{PinCode, PinError},
};
use stash::{
    orm::Model,
    stash::{StashError, Tether},
};

#[tokio::test]
async fn create_and_delete_pin() {
    let test_ctx = TestContext::new().await;
    let core_ctx = test_ctx.core_context();
    let pin = vec![1, 2, 3, 4];

    PinCode::set_pin(core_ctx.clone(), pin.clone())
        .await
        .unwrap();

    let mut tether = core_ctx.account_stash().connection();
    let app_settings = AppSettings::get_or_default(&tether).await;

    assert_eq!(app_settings.protection, AppProtection::Pin);
    let mut pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 0);

    pin_metadata.last_access_reset(&mut tether).await.unwrap();

    let incorrect_pin = vec![0, 0, 0, 0];

    let error = PinCode::delete_pin(core_ctx.clone(), incorrect_pin)
        .await
        .unwrap_err();

    assert!(matches!(error, PinError::IncorrectPin));

    pin_metadata.reload(&tether).await.unwrap();
    assert_eq!(pin_metadata.attempts, 1);

    pin_metadata.last_access_reset(&mut tether).await.unwrap();

    PinCode::delete_pin(core_ctx.clone(), pin).await.unwrap();

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

    PinCode::set_pin(core_ctx.clone(), pin.clone())
        .await
        .unwrap();

    let mut tether = core_ctx.account_stash().connection();
    let app_settings = AppSettings::get_or_default(&tether).await;

    assert_eq!(app_settings.protection, AppProtection::Pin);
    let mut pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 0);

    pin_metadata.last_access_reset(&mut tether).await.unwrap();

    PinCode::validate_pin(core_ctx.clone(), pin.clone())
        .await
        .unwrap();

    let new_pin = vec![0, 0, 0, 0];
    let old_pin = pin;

    // Lets create new pin
    PinCode::set_pin(core_ctx.clone(), new_pin.clone())
        .await
        .unwrap();

    pin_metadata.last_access_reset(&mut tether).await.unwrap();

    let error = PinCode::validate_pin(core_ctx.clone(), old_pin)
        .await
        .unwrap_err();
    assert!(matches!(error, PinError::IncorrectPin));

    pin_metadata.last_access_reset(&mut tether).await.unwrap();

    PinCode::validate_pin(core_ctx.clone(), new_pin)
        .await
        .unwrap();

    let count = PinProtection::count("", vec![], &tether).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn validation_max_attempts() {
    let test_ctx = TestContext::new().await;
    let core_ctx = test_ctx.core_context();
    let pin = vec![1, 2, 3, 4];
    let incorrect_pin = vec![0, 0, 0, 0];

    PinCode::set_pin(core_ctx.clone(), pin).await.unwrap();

    let mut tether = core_ctx.account_stash().connection();
    let app_settings = AppSettings::get_or_default(&tether).await;

    assert_eq!(app_settings.protection, AppProtection::Pin);
    let mut pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 0);

    // Lets pretend we reached the limit
    pin_metadata.attempts = PinCode::MAX_ATTEMPTS;
    pin_metadata.last_access_unixepoch = 0;

    tether
        .tx(async |tx| pin_metadata.save(tx).await)
        .await
        .unwrap();

    let error = PinCode::validate_pin(core_ctx.clone(), incorrect_pin.clone())
        .await
        .unwrap_err();

    assert!(matches!(error, PinError::TooManyAttempts));

    let mut pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 11);

    pin_metadata.last_access_reset(&mut tether).await.unwrap();

    // Pin code is not responsible to do anything regarding `TooManyAttempts`
    // error. In production flow there is catch on this error
    // which nukes databases and caches.
    let error = PinCode::validate_pin(core_ctx.clone(), incorrect_pin)
        .await
        .unwrap_err();

    assert!(matches!(error, PinError::TooManyAttempts));

    let pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 12);
}

trait PinProtectionExt {
    fn last_access_reset(
        &mut self,
        tether: &mut Tether,
    ) -> impl Future<Output = Result<(), StashError>>;
}

impl PinProtectionExt for PinProtection {
    async fn last_access_reset(&mut self, tether: &mut Tether) -> Result<(), StashError> {
        self.reload(tether).await?;
        self.last_access_unixepoch = 0;

        tether.tx(async |tx| self.save(tx).await).await?;

        Ok(())
    }
}
