use proton_core_common::{
    models::{AppProtection, AppSettings, Label, ModelExtension, PinProtection},
    pin_code::{PinCode, PinError},
};
use proton_core_test_utils::test_context::TestContext;
use stash::stash::{StashError, Tether};

#[tokio::test]
async fn create_and_delete_pin() {
    let test_ctx = TestContext::new().await;
    let core_ctx = test_ctx.core_context();
    let pin = vec![1, 2, 3, 4];

    PinCode::create_pin(core_ctx.clone(), pin.clone())
        .await
        .unwrap();

    let mut tether = core_ctx.account_stash().connection();
    let app_settings = AppSettings::get_or_default(&tether).await;

    assert_eq!(app_settings.protection, AppProtection::Pin);
    let mut pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();

    assert_eq!(pin_metadata.attempts, 0);

    let error = PinCode::delete_pin(core_ctx.clone(), pin.clone())
        .await
        .unwrap_err();

    assert!(matches!(error, PinError::TooFrequentAttempts));

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
async fn validation_max_attempts() {
    let test_ctx = TestContext::new().await;
    let core_ctx = test_ctx.core_context();
    let user_ctx = test_ctx.user_context().await;
    let pin = vec![1, 2, 3, 4];
    let incorrect_pin = vec![0, 0, 0, 0];

    PinCode::create_pin(core_ctx.clone(), pin).await.unwrap();

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

    let error = PinCode::validate_pin(core_ctx.clone(), incorrect_pin)
        .await
        .unwrap_err();

    assert!(matches!(error, PinError::TooManyAttempts));

    let error = PinProtection::get(&tether).await.unwrap_err();

    assert!(error.to_string().contains("no such table: pin_protection"));

    let tether = user_ctx.stash().connection();
    let error = Label::all(&tether).await.unwrap_err();

    assert!(error.to_string().contains("no such table: labels"));
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
