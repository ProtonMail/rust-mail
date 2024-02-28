use crate::DBResult;
use proton_api_core::domain::{
    ProtonBoolean, TFAStatus, User, UserId, UserLogAuth, UserProductUsedSpace, UserSettings,
    UserSettings2FA, UserSettingsDateFormat, UserSettingsDensity, UserSettingsEmail,
    UserSettingsFlags, UserSettingsHighSecurity, UserSettingsPassword, UserSettingsPhone,
    UserSettingsTimeFormat, UserSettingsWeekStart,
};
use proton_api_core::exports::crypto::domain::UserKeys;
use proton_api_core::exports::crypto::keyring::{KeyId, LockedKey};

#[cfg(test)]
fn new_core_test_connection() -> crate::CoreSqliteConnection {
    use crate::migrations::migrate_core_db;
    use proton_sqlite3::{InProcessTrackerService, SqliteConnectionPool, SqliteMode};
    let pool = SqliteConnectionPool::new(SqliteMode::InMemory, false);
    {
        let mut conn = pool.acquire().unwrap();
        migrate_core_db(&mut conn).unwrap();
    }
    let tracker = InProcessTrackerService::new(pool);
    tracker
        .new_connection()
        .expect("failed to acquire connection")
        .into()
}

#[test]
fn test_core_store_and_load_user() {
    let mut conn = new_core_test_connection();
    let user = User {
        id: UserId::from("my_user_id"),
        name: Some("my_user_name".to_string()),
        display_name: Some("My User Name".to_string()),
        email: "my_name@user.me".to_string(),
        used_space: 1024,
        max_space: 4096,
        max_upload: 512,
        user_type: proton_api_core::domain::UserType::Proton,
        create_time: 111111,
        credit: 222222,
        currency: "euro".to_string(),
        keys: UserKeys(vec![LockedKey {
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
        }]),
        product_used_space: UserProductUsedSpace {
            calender: 23,
            contact: 44,
            drive: 99,
            mail: 12,
            pass: 33,
        },
        to_migrate: Default::default(),
        mnemonic_status: proton_api_core::domain::UserMnemonicStatus::Disabled,
        role: 12345,
        private: 442424,
        subscribed: 3234234,
        services: 23123123,
        delinquent: 4,
        flags: 9124,
    };

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
fn test_core_store_and_load_user_settings() {
    let mut conn = new_core_test_connection();

    let user_id = UserId::from("USER");

    let settings = UserSettings {
        email: UserSettingsEmail {
            value: "FooBar".to_string(),
            status: 1,
            notify: 2,
            reset: 4,
        },
        password: UserSettingsPassword {
            mode: 2,
            expiration_time: 1034,
        },
        phone: UserSettingsPhone {
            value: "1234556".to_string(),
            status: 9,
            notify: 5,
            reset: 7,
        },
        two_factor_auth: UserSettings2FA {
            enabled: TFAStatus::FIDO2,
            allowed: TFAStatus::TotpOrFIDO2,
            expiration_time: 9999,
            registered_keys: vec![],
        },
        news: 111,
        locale: "LOCALE".to_string(),
        log_auth: UserLogAuth::Advanced,
        invoice_text: "my_invoice".to_string(),
        density: UserSettingsDensity::Compact,
        week_start: UserSettingsWeekStart::Sunday,
        date_format: UserSettingsDateFormat::YYYYMMDD,
        time_format: UserSettingsTimeFormat::H12,
        welcome: Default::default(),
        early_access: Default::default(),
        flags: UserSettingsFlags {
            welcomed: ProtonBoolean::True,
            in_app_promos_hidden: Default::default(),
        },
        referral: None,
        device_recovery: Default::default(),
        telemetry: ProtonBoolean::True,
        crash_reports: Default::default(),
        hide_side_panel: ProtonBoolean::True,
        high_security: UserSettingsHighSecurity {
            eligible: Default::default(),
            value: ProtonBoolean::True,
        },
        session_account_recovery: ProtonBoolean::True,
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
