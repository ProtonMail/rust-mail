use crate::common::TestContext;
use proton_api_mail::domain::{
    Address, AddressId, AddressStatus, AddressType, Conversation, ConversationCount,
    ConversationId, ConversationLabels, Label, LabelId, LabelType, MailSettings, MessageCount,
    ALL_LABEL_TYPES,
};
use proton_api_mail::exports::crypto::domain::{AddressKeys, UserKeys};
use proton_api_mail::proton_api_core::domain::{
    DateFormat, Density, Email, EventId, Flags, HighSecurity, LogAuth, Password, Phone,
    ProductUsedSpace, SettingsFlags, TFAStatus, TimeFormat, TwoFA, User, UserId,
    UserMnemonicStatus, UserSettings, UserType, WeekStart,
};
use proton_api_mail::proton_api_core::requests::{
    LatestEventResponse, UserInfoResponse, UserSettingsResponse,
};
use proton_api_mail::requests::{
    GetAddressesResponse, GetConversationCountsResponse, GetConversationsResponse,
    GetLabelsResponse, GetMessageCountsResponse, MailSettingsResponse,
};
use proton_mail_common::{
    MailContextError, MailUserContextInitializationCallback, MailUserContextLoadingStage,
};
use std::collections::HashMap;
use velcro::hash_map;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

/// Mail user context init callback that does nothing.
pub struct NullCallback {}

impl MailUserContextInitializationCallback for NullCallback {
    fn on_stage(&self, _: MailUserContextLoadingStage) {}

    fn on_stage_err(&self, _: MailUserContextLoadingStage, _: MailContextError) {}
}

/// Initialization parameters.
#[derive(Clone, Default)]
pub struct Params {
    /// Last event id. If `None`, it will be set to `0`.
    pub last_event_id: Option<EventId>,

    /// User info. If `None`, some default values will be set.
    pub user_info: Option<User>,

    /// User settings. If `None`, some default values will be set.
    pub user_settings: Option<UserSettings>,

    /// Mail settings. If `None`, some default values will be set.
    pub mail_settings: Option<MailSettings>,

    /// List of labels by type.
    pub labels: HashMap<LabelType, Vec<Label>>,

    /// List of user addresses.
    pub addresses: Vec<Address>,

    /// List of conversations.
    pub conversations: Vec<Conversation>,

    /// List of conversation counts.
    pub conversation_count: Vec<ConversationCount>,

    /// List of message counts.
    pub message_count: Vec<MessageCount>,
}

impl Params {
    /// Create a basic set of parameters with some default values.
    ///
    /// This function goes beyond the bare type defaults, and sets up some basic
    /// information to use to test against. Specifically, it sets up a single
    /// label, a single address, a single conversation, and the related counts.
    ///
    pub fn default_basic() -> Self {
        Self {
            last_event_id: None,
            user_info: None,
            user_settings: None,
            mail_settings: None,
            labels: hash_map! {
                LabelType::Label: vec![Label {
                    id: LabelId::from("mylabel"),
                    parent_id: None,
                    name: "mylabel".to_string(),
                    path: None,
                    color: "".to_string(),
                    label_type: LabelType::Label,
                    notify: false,
                    display: false,
                    sticky: false,
                    expanded: false,
                    order: 0,
                }]
            },
            addresses: vec![Address {
                id: AddressId::from("myaddress"),
                email: "foo@bar.com".to_string(),
                send: true,
                receive: true,
                status: AddressStatus::Enabled,
                domain_id: None,
                address_type: AddressType::Original,
                order: 0,
                display_name: "".to_string(),
                signature: "".to_string(),
                keys: AddressKeys(vec![]),
                catch_all: false,
                proton_mx: false,
                signed_key_list: Default::default(),
            }],
            conversations: vec![Conversation {
                id: ConversationId::from("myconv"),
                order: 0,
                subject: "Hello".to_string(),
                senders: vec![],
                recipients: vec![],
                num_messages: 1,
                num_unread: 0,
                num_attachments: 0,
                expiration_time: 0,
                size: 12,
                labels: vec![ConversationLabels {
                    id: LabelId::inbox().clone(),
                    context_num_unread: 0,
                    context_num_messages: 1,
                    context_time: 0,
                    context_size: 12,
                    context_num_attachments: 0,
                    context_expiration_time: 0,
                }],
                display_snooze_reminder: false,
                attachments_metadata: vec![],
                attachment_info: Default::default(),
            }],
            conversation_count: vec![ConversationCount {
                label_id: LabelId::inbox().clone(),
                total: 1,
                unread: 0,
            }],
            message_count: vec![MessageCount {
                label_id: LabelId::inbox().clone(),
                total: 1,
                unread: 0,
            }],
        }
    }
}

impl TestContext {
    /// Set up basic user data.
    ///
    /// This function sets up basic user data that should be fetched after login
    /// to initialize the database and/or the context for the tests.
    ///
    /// # Parameters
    ///
    /// * `params` - The parameters to use for the setup.
    ///
    pub async fn setup_user(&self, mut params: Params) {
        // Latest event id
        Mock::given(method("GET"))
            .and(path("/api/core/v4/events/latest"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(LatestEventResponse {
                    event_id: params.last_event_id.unwrap_or(EventId::from("0")),
                }),
            )
            .expect(1)
            .mount(self.mock_server())
            .await;

        // User info
        Mock::given(method("GET"))
            .and(path("/api/core/v4/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(UserInfoResponse {
                user: params.user_info.unwrap_or(User {
                    id: UserId::from("user"),
                    name: None,
                    display_name: None,
                    email: "".to_string(),
                    used_space: 0,
                    max_space: 0,
                    max_upload: 0,
                    user_type: UserType::Proton,
                    create_time: 0,
                    credit: 0,
                    currency: "".to_string(),
                    keys: UserKeys(vec![]),
                    product_used_space: ProductUsedSpace {
                        calendar: 0,
                        contact: 0,
                        drive: 0,
                        mail: 0,
                        pass: 0,
                    },
                    to_migrate: false,
                    mnemonic_status: UserMnemonicStatus::Disabled,
                    role: 0,
                    private: 0,
                    subscribed: 0,
                    services: 0,
                    delinquent: 0,
                    flags: Flags {
                        protected: false,
                        onboard_checklist_storage_granted: false,
                        has_temporary_password: false,
                        test_account: false,
                        no_login: false,
                        recovery_attempt: false,
                        sso: false,
                        no_proton_address: false,
                    },
                }),
            }))
            .expect(1)
            .mount(self.mock_server())
            .await;

        // User settings
        Mock::given(method("GET"))
            .and(path("/api/core/v4/settings"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(UserSettingsResponse {
                    user_settings: params.user_settings.unwrap_or(UserSettings {
                        email: Email {
                            value: "".to_string(),
                            status: 0,
                            notify: 0,
                            reset: 0,
                        },
                        password: Password {
                            mode: 0,
                            expiration_time: None,
                        },
                        phone: Phone {
                            value: "".to_string(),
                            status: 0,
                            notify: 0,
                            reset: 0,
                        },
                        two_factor_auth: TwoFA {
                            enabled: TFAStatus::None,
                            allowed: TFAStatus::None,
                            expiration_time: None,
                            registered_keys: vec![],
                        },
                        news: 0,
                        locale: "".to_string(),
                        log_auth: LogAuth::Disabled,
                        invoice_text: "".to_string(),
                        density: Density::Comfortable,
                        week_start: WeekStart::Default,
                        date_format: DateFormat::Default,
                        time_format: TimeFormat::Default,
                        welcome: false,
                        early_access: false,
                        flags: SettingsFlags {
                            welcomed: false,
                            in_app_promos_hidden: false,
                        },
                        referral: None,
                        device_recovery: false,
                        telemetry: false,
                        crash_reports: false,
                        hide_side_panel: false,
                        high_security: HighSecurity {
                            eligible: false,
                            value: false,
                        },
                        session_account_recovery: false,
                    }),
                }),
            )
            .expect(1)
            .mount(self.mock_server())
            .await;

        // Mail settings
        Mock::given(method("GET"))
            .and(path("/api/mail/v4/settings"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(MailSettingsResponse {
                    mail_settings: params.mail_settings.unwrap_or(MailSettings {
                        display_name: "".to_string(),
                        signature: "".to_string(),
                        theme: "".to_string(),
                        auto_save_contacts: false,
                        composer_mode: Default::default(),
                        message_buttons: Default::default(),
                        show_images: Default::default(),
                        show_moved: Default::default(),
                        auto_delete_spam_and_trash_days: None,
                        almost_all_mail: Default::default(),
                        next_message_on_move: None,
                        view_mode: Default::default(),
                        view_layout: Default::default(),
                        swipe_left: Default::default(),
                        swipe_right: Default::default(),
                        shortcuts: false,
                        pm_signature: Default::default(),
                        pm_signature_referral_link: false,
                        image_proxy: 0,
                        num_message_per_page: 0,
                        draft_mime_type: "".to_string(),
                        receive_mime_type: "".to_string(),
                        show_mime_type: "".to_string(),
                        enable_folder_color: false,
                        inherit_parent_folder_color: false,
                        submission_access: false,
                        right_to_left: Default::default(),
                        attach_public_key: false,
                        sign: false,
                        pgp_scheme: Default::default(),
                        prompt_pin: false,
                        sticky_labels: false,
                        confirm_link: false,
                        delay_send_seconds: 0,
                        font_face: None,
                        spam_action: None,
                        block_sender_confirmation: None,
                        mobile_settings: None,
                        hide_remote_images: false,
                        hide_sender_images: false,
                    }),
                }),
            )
            .expect(1)
            .mount(self.mock_server())
            .await;

        // Mail addresses
        Mock::given(method("GET"))
            .and(path("/api/core/v4/addresses"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetAddressesResponse {
                    addresses: params.addresses,
                }),
            )
            .expect(1)
            .mount(self.mock_server())
            .await;

        // Labels
        for label_type in ALL_LABEL_TYPES {
            let labels = params.labels.remove(&label_type).unwrap_or_default();
            let resp = GetLabelsResponse { labels };

            Mock::given(method("GET"))
                .and(path("/api/core/v4/labels"))
                .and(query_param("Type", (label_type as u8).to_string()))
                .respond_with(ResponseTemplate::new(200).set_body_json(resp))
                .expect(1)
                .mount(self.mock_server())
                .await;
        }

        // Message counts
        Mock::given(method("GET"))
            .and(path("/api/mail/v4/messages/count"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetMessageCountsResponse {
                    counts: params.message_count,
                }),
            )
            .expect(1)
            .mount(self.mock_server())
            .await;

        // Conversation counts
        Mock::given(method("GET"))
            .and(path("/api/mail/v4/conversations/count"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationCountsResponse {
                    counts: params.conversation_count,
                }),
            )
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving conversations.
    ///
    /// This function will mock the response for the given conversations.
    ///
    /// # Parameters
    ///
    /// * `conversations` - The list of conversations to respond with.
    /// * `expect`        - How many times the endpoint should be called.
    ///
    pub async fn mock_get_conversations(&self, conversations: Vec<Conversation>, expect: u64) {
        Mock::given(method("GET"))
            .and(path("/api/mail/v4/conversations"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                    conversations,
                    stale: false,
                    total: 1,
                }),
            )
            .expect(expect)
            .mount(self.mock_server())
            .await;
    }
}
