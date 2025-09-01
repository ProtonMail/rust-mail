use crate::datatypes::{
    DateFormat, Density, Email, Flags, HighSecurity, LogAuth, Password, Phone, ProductUsedSpace,
    SettingsFlags, TfaStatus, TimeFormat, TwoFa, UserKeys, UserMnemonicStatus, UserType, WeekStart,
};
use crate::models::{DelinquentState, PaidSubscription, User, UserSettings};
use crate::tests::common::new_core_test_connection;
use proton_core_api::services::proton::UserId;
use proton_crypto_account::keys::{ArmoredPrivateKey, KeyId, LockedKey, UserKeys as RealUserKeys};
use stash::orm::Model;
use stash::stash::StashError;

#[tokio::test]
async fn test_core_store_and_load_user() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut user = new_test_user();
            user.save(tx).await.expect("failed to store user");
            let db_user = User::load(user.remote_id.clone().unwrap(), tx)
                .await
                .expect("failed to load user")
                .expect("should have value");
            assert_eq!(db_user, user);
            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_core_user_space_updates() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    let mut user = new_test_user();
    tether
        .tx::<_, _, StashError>(async |tx| {
            user.save(tx).await.expect("failed to store user");

            user.used_space = 912_314_142;
            user.save(tx).await.expect("failed to update used space");

            user.product_used_space = ProductUsedSpace {
                calendar: 234_235_235_235,
                contact: 2_342_342_111_231,
                drive: 32_423_487_767_455,
                mail: 10_202_042_014,
                pass: 1_234_857_671,
            };

            user.save(tx).await.expect("failed to update used space");

            let db_user = User::load(user.remote_id.clone().unwrap(), tx)
                .await
                .expect("failed to load user")
                .expect("should have value");
            assert_eq!(db_user, user);
            Ok(())
        })
        .await
        .unwrap();
}
#[tokio::test]
async fn test_core_store_and_load_user_settings() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();

    let user_id = UserId::from("USER");

    let mut settings = UserSettings {
        remote_id: Some(user_id.clone()),
        email: Email {
            value: "FooBar".to_owned(),
            status: 1,
            notify: 2,
            reset: 4,
        },
        password: Password {
            mode: 2,
            expiration_time: Some(1034),
        },
        phone: Phone {
            value: "1234556".to_owned(),
            status: 9,
            notify: 5,
            reset: 7,
        },
        two_factor_auth: TwoFa {
            enabled: TfaStatus::Fido2,
            allowed: TfaStatus::TotpOrFido2,
            expiration_time: Some(9999),
            registered_keys: vec![],
        },
        news: 111,
        locale: "LOCALE".to_owned(),
        log_auth: LogAuth::Advanced,
        invoice_text: "my_invoice".to_owned(),
        density: Density::Compact,
        week_start: WeekStart::Sunday,
        date_format: DateFormat::YyyyMmDd,
        time_format: TimeFormat::H12,
        welcome: Default::default(),
        early_access: Default::default(),
        flags: SettingsFlags {
            welcomed: true,
            edm_opt_out: false,
        },
        referral: None,
        device_recovery: Default::default(),
        telemetry: true,
        crash_reports: Default::default(),
        hide_side_panel: true,
        high_security: HighSecurity {
            eligible: Default::default(),
            value: true,
        },
        session_account_recovery: true,
    };

    tether
        .tx::<_, _, StashError>(async |tx| {
            settings.save(tx).await.expect("failed to store settings");
            let db_settings = UserSettings::load(user_id.clone(), tx)
                .await
                .expect("failed to load user")
                .expect("should have value");
            assert_eq!(db_settings, settings);
            Ok(())
        })
        .await
        .unwrap();
}

fn new_test_user() -> User {
    User {
        remote_id: Some(UserId::from("my_user_id")),
        name: Some("my_user_name".to_owned()),
        display_name: Some("My User Name".to_owned()),
        email: "my_name@user.me".to_owned(),
        used_space: 1024,
        max_space: 4096,
        max_upload: 512,
        user_type: UserType::Proton,
        create_time: 111_111.into(),
        credit: 222_222,
        currency: "euro".to_owned(),
        keys: UserKeys(RealUserKeys(vec![
            LockedKey {
                id: KeyId::from("My_key_id"),
                version: 3,
                private_key: ArmoredPrivateKey::from("my_private_key".to_owned()),
                token: None,
                signature: None,
                activation: None,
                primary: true,
                active: false,
                flags: None,
                recovery_secret: Some("recovery_secret".to_owned()),
                recovery_secret_signature: Some("recovery_signature".to_owned()),
                address_forwarding_id: None,
            },
            LockedKey {
                id: KeyId::from("My_key_id2"),
                version: 3,
                private_key: ArmoredPrivateKey::from("my_private_key2".to_owned()),
                token: None,
                signature: None,
                activation: None,
                primary: true,
                active: false,
                flags: None,
                recovery_secret: Some("recovery_secret2".to_owned()),
                recovery_secret_signature: Some("recovery_signature2".to_owned()),
                address_forwarding_id: None,
            },
        ])),
        product_used_space: ProductUsedSpace {
            calendar: 23,
            contact: 44,
            drive: 99,
            mail: 12,
            pass: 33,
        },
        to_migrate: Default::default(),
        mnemonic_status: UserMnemonicStatus::Disabled,
        role: 12345,
        private: false,
        subscribed: PaidSubscription(3_234_234),
        services: 23_123_123,
        delinquent: DelinquentState::Paid,
        flags: Flags {
            protected: false,
            onboard_checklist_storage_granted: true,
            has_temporary_password: false,
            test_account: true,
            no_login: false,
            recovery_attempt: true,
            sso: false,
            no_proton_address: true,
        },
    }
}
