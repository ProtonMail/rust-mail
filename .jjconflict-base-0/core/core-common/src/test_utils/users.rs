use crate::models::PaidSubscription;
use crate::test_utils::account::{TEST_USER_ID, TEST_USER_MAIL, testdata_user_keys};
use crate::test_utils::test_context::TestContext;
use proton_core_api::services::proton::{
    DateFormat as ApiDateFormat, DelinquentState, Density as ApiDensity, Email as ApiEmail,
    Flags as ApiFlags, GetSettingsResponse as GetCoreSettingsResponse, GetUsersResponse,
    HighSecurity as ApiHighSecurity, LogAuth as ApiLogAuth, Password as ApiPassword,
    PasswordMode as ApiPasswordMode, Phone as ApiPhone, ProductUsedSpace as ApiProductUsedSpace,
    Role as ApiRole, SettingsFlags as ApiSettingsFlags, TfaStatus as ApiTfaStatus,
    TimeFormat as ApiTimeFormat, TwoFa as ApiTwoFa, User as ApiUser, UserId,
    UserMnemonicStatus as ApiUserMnemonicStatus, UserSettings as ApiUserSettings,
    UserType as ApiUserType, WeekStart as ApiWeekStart,
};
use wiremock::Times;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

impl TestContext {
    #[function_name::named]
    pub async fn mock_get_user_settings(
        &self,
        settings: Option<ApiUserSettings>,
        expect: impl Into<Times>,
    ) {
        Mock::given(method("GET"))
            .and(path("/api/core/v4/settings"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetCoreSettingsResponse {
                    user_settings: settings.unwrap_or_else(DEFAULT_USER_SETTINGS),
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_get_user(&self, user: Option<ApiUser>, expect: impl Into<Times>) {
        Mock::given(method("GET"))
            .and(path("/api/core/v4/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(GetUsersResponse {
                user: user.unwrap_or_else(DEFAULT_USER),
            }))
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}

pub const DEFAULT_USER_SETTINGS: fn() -> ApiUserSettings = || ApiUserSettings {
    email: ApiEmail {
        value: String::new(),
        status: 0,
        notify: 0,
        reset: 0,
    },
    password: ApiPassword {
        mode: ApiPasswordMode::One,
        expiration_time: None,
    },
    phone: ApiPhone {
        value: String::new(),
        status: 0,
        notify: 0,
        reset: 0,
    },
    two_factor_auth: ApiTwoFa {
        enabled: ApiTfaStatus::None,
        allowed: ApiTfaStatus::None,
        expiration_time: None,
        registered_keys: vec![],
    },
    news: 0,
    locale: String::new(),
    log_auth: ApiLogAuth::Disabled,
    invoice_text: String::new(),
    density: ApiDensity::Comfortable,
    week_start: ApiWeekStart::Default,
    date_format: ApiDateFormat::Default,
    time_format: ApiTimeFormat::Default,
    welcome: false,
    early_access: false,
    flags: ApiSettingsFlags {
        welcomed: false,
        edm_opt_out: false,
    },
    referral: None,
    device_recovery: false,
    telemetry: false,
    crash_reports: false,
    hide_side_panel: false,
    high_security: ApiHighSecurity {
        eligible: false,
        value: false,
    },
    session_account_recovery: false,
};

pub const DEFAULT_USER: fn() -> ApiUser = || ApiUser {
    id: UserId::from(TEST_USER_ID),
    name: None,
    display_name: None,
    email: TEST_USER_MAIL.to_owned(),
    used_space: 0,
    max_space: 0,
    max_upload: 0,
    user_type: ApiUserType::Proton,
    create_time: 0,
    credit: 0,
    currency: String::new(),
    keys: testdata_user_keys(),
    product_used_space: ApiProductUsedSpace {
        calendar: 0,
        contact: 0,
        drive: 0,
        mail: 0,
        pass: 0,
    },
    to_migrate: false,
    mnemonic_status: ApiUserMnemonicStatus::Disabled,
    role: ApiRole::None,
    private: false,
    subscribed: PaidSubscription::MAIL.0,
    services: 0,
    delinquent: DelinquentState::Paid,
    flags: ApiFlags {
        protected: false,
        onboard_checklist_storage_granted: false,
        has_temporary_password: false,
        test_account: false,
        no_login: false,
        recovery_attempt: false,
        sso: false,
        no_proton_address: false,
        has_a_byoe_address: false,
    },
};
