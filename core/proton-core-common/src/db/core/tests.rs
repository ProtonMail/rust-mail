use crate::db::{new_core_test_connection, DBResult};
use proton_api_core::domain::{
    DateFormat, Density, Email, Flags, HighSecurity, LogAuth, Password, Phone, ProductUsedSpace,
    SettingsFlags, TFAStatus, TimeFormat, TwoFA, User, UserId, UserSettings, WeekStart,
};
use proton_api_core::exports::crypto::domain::{KeyId, LockedKey, UserKeys};

#[test]
fn test_core_store_and_load_user() {
    let mut conn = new_core_test_connection();
    let user = new_test_user();
    conn.tx(|tx| -> DBResult<()> {
        tx.create_or_update_user(&user)
            .expect("failed to store user");
        let db_user = tx
            .get_user(&user.id)
            .expect("failed to load user")
            .expect("should have value");
        assert_eq!(db_user, user);
        Ok(())
    })
    .unwrap();
}

#[test]
fn test_core_user_space_updates() {
    let mut conn = new_core_test_connection();
    let mut user = new_test_user();
    conn.tx(|tx| -> DBResult<()> {
        tx.create_or_update_user(&user)
            .expect("failed to store user");

        user.used_space = 912_314_142;
        tx.update_user_used_space(&user.id, user.used_space)
            .expect("failed to update used space");

        user.product_used_space = ProductUsedSpace {
            calendar: 234_235_235_235,
            contact: 2_342_342_111_231,
            drive: 32_423_487_767_455,
            mail: 10_202_042_014,
            pass: 1_234_857_671,
        };

        tx.update_user_product_used_space(&user.id, &user.product_used_space)
            .expect("failed to update used space");

        let db_user = tx
            .get_user(&user.id)
            .expect("failed to load user")
            .expect("should have value");
        assert_eq!(db_user, user);
        Ok(())
    })
    .unwrap();
}
#[test]
fn test_core_store_and_load_user_settings() {
    let mut conn = new_core_test_connection();

    let user_id = UserId::from("USER");

    let settings = UserSettings {
        email: Email {
            value: "FooBar".to_string(),
            status: 1,
            notify: 2,
            reset: 4,
        },
        password: Password {
            mode: 2,
            expiration_time: Some(1034),
        },
        phone: Phone {
            value: "1234556".to_string(),
            status: 9,
            notify: 5,
            reset: 7,
        },
        two_factor_auth: TwoFA {
            enabled: TFAStatus::FIDO2,
            allowed: TFAStatus::TotpOrFIDO2,
            expiration_time: Some(9999),
            registered_keys: vec![],
        },
        news: 111,
        locale: "LOCALE".to_string(),
        log_auth: LogAuth::Advanced,
        invoice_text: "my_invoice".to_string(),
        density: Density::Compact,
        week_start: WeekStart::Sunday,
        date_format: DateFormat::YYYYMMDD,
        time_format: TimeFormat::H12,
        welcome: Default::default(),
        early_access: Default::default(),
        flags: SettingsFlags {
            welcomed: true,
            in_app_promos_hidden: Default::default(),
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

    conn.tx(|tx| -> DBResult<()> {
        tx.create_or_update_user_settings(&user_id, &settings)
            .expect("failed to store settings");
        let db_settings = tx
            .get_user_settings(&user_id)
            .expect("failed to load settings")
            .unwrap();
        assert_eq!(db_settings, settings);
        Ok(())
    })
    .unwrap();
}

fn new_test_user() -> User {
    User {
        id: UserId::from("my_user_id"),
        name: Some("my_user_name".to_string()),
        display_name: Some("My User Name".to_string()),
        email: "my_name@user.me".to_string(),
        used_space: 1024,
        max_space: 4096,
        max_upload: 512,
        user_type: proton_api_core::domain::UserType::Proton,
        create_time: 111_111,
        credit: 222_222,
        currency: "euro".to_string(),
        keys: UserKeys(vec![
            LockedKey {
                id: KeyId::from("My_key_id"),
                version: 3,
                private_key: "my_private_key".to_string(),
                token: None,
                signature: None,
                activation: None,
                primary: true,
                active: false,
                flags: None,
                recovery_secret: Some("recovery_secret".to_string()),
                recovery_secret_signature: Some("recovery_signature".to_string()),
                address_forwarding_id: None,
            },
            LockedKey {
                id: KeyId::from("My_key_id2"),
                version: 3,
                private_key: "my_private_key2".to_string(),
                token: None,
                signature: None,
                activation: None,
                primary: true,
                active: false,
                flags: None,
                recovery_secret: Some("recovery_secret2".to_string()),
                recovery_secret_signature: Some("recovery_signature2".to_string()),
                address_forwarding_id: None,
            },
        ]),
        product_used_space: ProductUsedSpace {
            calendar: 23,
            contact: 44,
            drive: 99,
            mail: 12,
            pass: 33,
        },
        to_migrate: Default::default(),
        mnemonic_status: proton_api_core::domain::UserMnemonicStatus::Disabled,
        role: 12345,
        private: 442_424,
        subscribed: 3_234_234,
        services: 23_123_123,
        delinquent: 4,
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
