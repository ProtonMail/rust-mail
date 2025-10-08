use super::attachment::{testdata_attachment_metadata, testdata_attachment_metadata_complete};
use crate::test_utils::test_context::MailTestContext;
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressSignedKeyList, AddressStatus as ApiAddressStatus,
    AddressType as ApiAddressType, ContactBasic as ApiContactBasic,
    ContactEmail as ApiContactEmail, Label as ApiLabel, User as ApiUser,
    UserSettings as ApiUserSettings,
};
use proton_core_api::services::proton::{AddressId, EventId, LabelId, LabelType as ApiLabelType};
use proton_core_api::services::proton::{GetEventsLatestResponse, GetKeysAllResponse};
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::GetIncomingDefaultResponse;
use proton_mail_api::services::proton::request_data::{
    PutMobileSettings, PutNextMessageOnMoveRequest,
};
use proton_mail_api::services::proton::response_data::MessageMetadata;
use proton_mail_api::services::proton::response_data::{
    AlmostAllMail, Attachment as ApiAttachment, ComposerDirection, ComposerMode,
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    ConversationLabel as ApiConversationLabel, IncomingDefault, MailSettings as ApiMailSettings,
    MessageButtons, MessageCount as ApiMessageCount, MessageMetadata as ApiMessageMetadata,
    MessageSender as ApiMessageSender, MimeType, PgpScheme, PmSignature, ShowImages, ShowMoved,
    SwipeAction, ViewLayout, ViewMode,
};

use crate::datatypes::SystemLabelId;
use proton_core_common::datatypes::ALL_LABEL_TYPES;
use proton_core_common::test_utils::account::{
    TEST_ADDRESS_ID, TEST_ADDRESS_KEY_SIGNATURE, TEST_USER_MAIL,
    testdata_address_keys_for_user_address,
};
use proton_core_common::test_utils::addresses_public::{
    TEST_OTHER_USER_EMAIL, testdata_address_keys_other_user,
};
use proton_mail_api::services::proton::responses::{
    GetConversationResponse, GetConversationsCountResponse, GetConversationsResponse,
    GetMailSettingsResponse, GetMessagesCountResponse, GetMessagesResponse,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;
use velcro::hash_map;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockBuilder, ResponseTemplate, Times};

/// Initialization parameters.
#[derive(Clone, Default)]
pub struct Params {
    /// Last event id. If `None`, it will be set to `0`.
    pub last_event_id: Option<EventId>,

    /// User info. If `None`, some default values will be set.
    pub user_info: Option<ApiUser>,

    /// User settings. If `None`, some default values will be set.
    pub user_settings: Option<ApiUserSettings>,

    /// List of user addresses.
    pub addresses: Vec<ApiAddress>,

    /// Keys from other users by email in format (email, key response).
    pub recipient_keys: Vec<(String, GetKeysAllResponse)>,

    /// Mail settings. If `None`, some default values will be set.
    pub mail_settings: Option<ApiMailSettings>,

    /// List of labels by type.
    pub labels: HashMap<ApiLabelType, Vec<ApiLabel>>,

    /// List of conversations.
    pub conversations: Vec<ApiConversation>,

    /// List of attachments.
    pub attachments: Vec<ApiAttachment>,

    /// List of conversation counts.
    pub conversation_count: Vec<ApiConversationCount>,

    /// List of message counts.
    pub message_count: Vec<ApiMessageCount>,

    /// List of contacts
    pub contacts: Vec<ApiContactBasic>,

    /// List of email contacts
    pub emails: Vec<ApiContactEmail>,
}

impl Params {
    /// Create a basic set of parameters with some default values.
    ///
    /// This function goes beyond the bare type defaults, and sets up some basic
    /// information to use to test against. Specifically, it sets up a single
    /// label, a single address, a single conversation, and the related counts.
    ///
    #[must_use]
    pub fn default_basic() -> Self {
        Self {
            last_event_id: None,
            user_info: None,
            user_settings: None,
            mail_settings: None,
            labels: hash_map! {
                ApiLabelType::Label: vec![ApiLabel {
                    id: LabelId::from("mylabel"),
                    parent_id: None,
                    name: "mylabel".to_owned(),
                    path: None,
                    color: String::new(),
                    label_type: ApiLabelType::Label,
                    notify: false,
                    display: false,
                    sticky: false,
                    expanded: false,
                    order: 0,
                }]
            },
            addresses: vec![ApiAddress {
                id: AddressId::from(TEST_ADDRESS_ID),
                email: TEST_USER_MAIL.to_owned(),
                send: true,
                receive: true,
                status: ApiAddressStatus::Enabled,
                domain_id: None,
                address_type: ApiAddressType::Original,
                order: 0,
                display_name: String::new(),
                signature: TEST_ADDRESS_KEY_SIGNATURE.to_owned(),
                keys: testdata_address_keys_for_user_address(),
                catch_all: false,
                proton_mx: false,
                signed_key_list: AddressSignedKeyList::default(),
            }],
            recipient_keys: vec![(
                TEST_OTHER_USER_EMAIL.to_owned(),
                testdata_address_keys_other_user(),
            )],
            conversations: vec![ApiConversation {
                id: ConversationId::from("myconv"),
                order: 0,
                subject: "Hello".to_owned(),
                senders: vec![ApiMessageSender {
                    address: "jsmith@test.com".into(),
                    name: "John Smith".into(),
                    is_proton: true,
                    display_sender_image: true,
                    is_simple_login: false,
                    bimi_selector: None,
                }],
                recipients: vec![],
                num_messages: 1,
                num_unread: 0,
                num_attachments: 0,
                expiration_time: 0,
                size: 12,
                labels: vec![ApiConversationLabel {
                    id: LabelId::inbox(),
                    context_num_unread: 0,
                    context_num_messages: 1,
                    context_time: 0,
                    context_size: 12,
                    context_num_attachments: 0,
                    context_expiration_time: 0,
                    context_snooze_time: 0,
                }],
                display_snoozed_reminder: false,
                attachments_metadata: vec![testdata_attachment_metadata()],
                attachment_info: BTreeMap::default(),
                context_time: None,
            }],
            attachments: vec![testdata_attachment_metadata_complete(
                MessageId::from("mymessage "),
                ConversationId::from("myconv"),
            )],
            conversation_count: vec![ApiConversationCount {
                label_id: LabelId::inbox(),
                total: 1,
                unread: 0,
            }],
            message_count: vec![ApiMessageCount {
                label_id: LabelId::inbox(),
                total: 1,
                unread: 0,
            }],
            contacts: vec![],
            emails: vec![],
        }
    }
}

impl MailTestContext {
    /// Set up basic user data.
    ///
    /// This function sets up basic user data that should be fetched after login
    /// to initialize the database and/or the context for the tests.
    ///
    pub async fn setup_user(&self, params: Params) {
        self.setup_user_repeated(params, 1).await;
    }

    /// Set up basic user data.
    ///
    /// This function sets up basic user data that should be fetched after login
    /// to initialize the database and/or the context for the tests.
    ///
    #[allow(clippy::too_many_lines)]
    pub async fn setup_user_repeated(&self, mut params: Params, number_of_calls: u64) {
        // Latest event id
        Mock::given(method("GET"))
            .and(path("/api/core/v4/events/latest"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetEventsLatestResponse {
                    event_id: params.last_event_id.unwrap_or(EventId::from("0")),
                }),
            )
            .expect(1) // this should only ever be initialized once at the moment
            .named("Setup user get latest events")
            .mount(self.mock_server())
            .await;

        // User info
        self.mock_get_user(params.user_info, number_of_calls).await;

        // User settings
        self.mock_get_user_settings(params.user_settings, number_of_calls)
            .await;

        // Mail settings
        self.mock_get_mail_settings(params.mail_settings, number_of_calls)
            .await;

        // Mail addresses
        self.core_test_context
            .mock_get_addresses(Some(params.addresses), number_of_calls)
            .await;

        // Incoming defaults
        self.mock_get_incoming_defaults(Some(vec![]), number_of_calls)
            .await;

        // Labels
        for label_type in ALL_LABEL_TYPES {
            let labels = params.labels.remove(&label_type.into()).unwrap_or_default();
            self.mock_get_labels_and(
                labels,
                |mock| mock.and(query_param("Type", (label_type as u8).to_string())),
                number_of_calls,
            )
            .await;
        }

        self.core_test_context
            .mock_get_contacts_emails(Some(params.emails), number_of_calls)
            .await;
        self.core_test_context
            .mock_get_contacts(Some(params.contacts), number_of_calls)
            .await;

        // Message counts
        self.mock_get_messages_count(Some(params.message_count), number_of_calls)
            .await;

        // Conversation counts
        self.mock_get_conversations_count(Some(params.conversation_count), number_of_calls)
            .await;

        for (email, response) in params.recipient_keys {
            self.core_test_context
                .mock_get_keys_all(&email, response)
                .await;
        }

        self.mock_ping_success().await;
    }

    /// Generate new mock expectations for retrieving conversations.
    ///
    /// This function will mock the response for the given conversations.
    ///
    #[function_name::named]
    pub async fn mock_get_conversations(
        &self,
        conversations: Vec<ApiConversation>,
        expect: impl Into<Times>,
    ) {
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
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving conversations.
    ///
    /// This function will mock the response for the given conversations.
    ///
    #[function_name::named]
    pub async fn mock_get_conversations_and(
        &self,
        conversations: Vec<ApiConversation>,
        and: impl Fn(Mock) -> Mock,
        expect: impl Into<Times>,
    ) {
        and(Mock::given(method("GET"))
            .and(path("/api/mail/v4/conversations"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                    total: conversations.len() as u64,
                    conversations,
                    stale: false,
                }),
            ))
        .expect(expect)
        .named(function_name!())
        .mount(self.mock_server())
        .await;
    }

    /// Generate new mock expectations for retrieving conversations.
    ///
    /// This function will mock the response for the given conversations.
    ///
    #[function_name::named]
    pub async fn mock_get_conversations_with(&self, with: impl Fn(MockBuilder) -> Mock) {
        with(Mock::given(method("GET")).and(path("/api/mail/v4/conversations")))
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for `ping` request
    ///
    /// This function will mock the response any ping request, returning 200.
    ///
    pub async fn mock_ping_success(&self) {
        Mock::given(method("GET"))
            .and(path("/api/core/v4/tests/ping"))
            .respond_with(ResponseTemplate::new(200))
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving conversations pages.
    ///
    /// This function will mock the response for the given conversations.
    ///
    #[function_name::named]
    pub async fn mock_get_conversations_page(
        &self,
        conversations: Vec<ApiConversation>,
        end_id: Option<ConversationId>,
        end_time: Option<u64>,
        page_size: u64,
        total: u64,
        expect: u64,
    ) {
        let mut mock = Mock::given(method("GET")).and(path("/api/mail/v4/conversations"));

        if let Some(id) = end_id {
            mock = mock.and(query_param("EndID", id.to_string()));
        }
        if let Some(time) = end_time {
            mock = mock.and(query_param("End", time.to_string()));
        }

        mock.and(query_param("PageSize", page_size.to_string()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                    conversations,
                    stale: false,
                    total,
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving message metadata.
    ///
    /// This function will mock the response for the given message metadata.
    ///
    #[function_name::named]
    pub async fn mock_get_message_metadata(
        &self,
        metadata: Vec<MessageMetadata>,
        expect: impl Into<Times>,
    ) {
        Mock::given(method("GET"))
            .and(path("/api/mail/v4/messages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
                    messages: metadata,
                    stale: false,
                    total: 1,
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving message metadata.
    ///
    /// This function will mock the response for the given message metadata.
    ///
    #[function_name::named]
    pub async fn mock_get_message_metadata_and(
        &self,
        metadata: Vec<MessageMetadata>,
        and: impl Fn(Mock) -> Mock,
        expect: impl Into<Times>,
    ) {
        and(Mock::given(method("GET"))
            .and(path("/api/mail/v4/messages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
                    total: metadata.len() as u64,
                    messages: metadata,
                    stale: false,
                }),
            ))
        .expect(expect)
        .named(function_name!())
        .mount(self.mock_server())
        .await;
    }

    /// Generate new mock expectations for retrieving message metadata pages.
    ///
    /// This function will mock the response for the given message metadata.
    ///
    #[function_name::named]
    pub async fn mock_get_message_metadata_page(
        &self,
        metadata: Vec<MessageMetadata>,
        end_id: Option<MessageId>,
        end_time: Option<u64>,
        page_size: u64,
        _total: u64,
        expect: u64,
    ) {
        let mut mock = Mock::given(method("GET")).and(path("/api/mail/v4/messages"));

        if let Some(id) = end_id {
            mock = mock.and(query_param("EndID", id.to_string()));
        }
        if let Some(time) = end_time {
            mock = mock.and(query_param("End", time.to_string()));
        }

        mock.and(query_param("PageSize", page_size.to_string()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
                    messages: metadata,
                    stale: false,
                    total: 1,
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving conversation's messages.
    ///
    /// This function will mock the response for the given conversations.
    ///
    #[function_name::named]
    pub async fn mock_get_conversation_messages(
        &self,
        conversation: ApiConversation,
        messages: Vec<ApiMessageMetadata>,
        expect: u64,
    ) {
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/mail/v4/conversations/{}",
                conversation.id
            )))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationResponse {
                    conversation,
                    messages,
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_get_mail_settings(
        &self,
        settings: Option<ApiMailSettings>,
        expect: impl Into<Times>,
    ) {
        Mock::given(method("GET"))
            .and(path("/api/mail/v4/settings"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetMailSettingsResponse {
                    mail_settings: settings.unwrap_or_else(|| DEFAULT_MAIL_SETTINGS.clone()),
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving message counts.
    ///
    /// This function will mock the response for the given message counts.
    ///
    #[function_name::named]
    pub async fn mock_get_messages_count(
        &self,
        counts: Option<Vec<ApiMessageCount>>,
        expect: impl Into<Times>,
    ) {
        let counts = counts.unwrap_or_default();
        Mock::given(method("GET"))
            .and(path("/api/mail/v4/messages/count"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetMessagesCountResponse { counts }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving conversation counts.
    ///
    /// This function will mock the response for the given conversation counts.
    ///
    #[function_name::named]
    pub async fn mock_get_conversations_count(
        &self,
        counts: Option<Vec<ApiConversationCount>>,
        expect: impl Into<Times>,
    ) {
        let counts = counts.unwrap_or_default();
        Mock::given(method("GET"))
            .and(path("/api/mail/v4/conversations/count"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationsCountResponse { counts }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving incoming defaults.
    ///
    /// This function will mock the response for the given incoming defaults.
    ///
    #[function_name::named]
    pub async fn mock_get_incoming_defaults(
        &self,
        incoming_defaults: Option<Vec<IncomingDefault>>,
        expect: impl Into<Times>,
    ) {
        let incoming_defaults = incoming_defaults.unwrap_or_default();
        Mock::given(method("GET"))
            .and(path("/api/mail/v4/incomingdefaults"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetIncomingDefaultResponse {
                    total: incoming_defaults.len() as u64,
                    global_total: incoming_defaults.len() as u64,
                    incoming_defaults,
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Helper function to create and mount mobile settings API mock
    #[function_name::named]
    pub async fn mock_put_mobile_settings(
        &self,
        response: ResponseTemplate,
        expected_payload: PutMobileSettings,
        expect: impl Into<Times>,
    ) {
        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/settings/mobilesettings"))
            .and(body_json(&expected_payload))
            .respond_with(response)
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_put_next_message_on_move(
        &self,
        response: ResponseTemplate,
        expected_payload: PutNextMessageOnMoveRequest,
        expect: impl Into<Times>,
    ) {
        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/settings/next-message-on-move"))
            .and(body_json(&expected_payload))
            .respond_with(response)
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}

pub static DEFAULT_MAIL_SETTINGS: LazyLock<ApiMailSettings> = LazyLock::new(|| ApiMailSettings {
    display_name: String::new(),
    signature: String::new(),
    theme: String::new(),
    auto_save_contacts: false,
    composer_mode: ComposerMode::default(),
    message_buttons: MessageButtons::default(),
    show_images: ShowImages::default(),
    show_moved: ShowMoved::default(),
    auto_delete_spam_and_trash_days: None,
    almost_all_mail: AlmostAllMail::default(),
    next_message_on_move: None,
    view_mode: ViewMode::default(),
    view_layout: ViewLayout::default(),
    swipe_left: SwipeAction::default(),
    swipe_right: SwipeAction::default(),
    shortcuts: false,
    pm_signature: PmSignature::default(),
    pm_signature_referral_link: false,
    image_proxy: 0,
    num_message_per_page: 0,
    draft_mime_type: MimeType::default(),
    receive_mime_type: MimeType::default(),
    show_mime_type: MimeType::default(),
    enable_folder_color: false,
    inherit_parent_folder_color: false,
    submission_access: false,
    right_to_left: ComposerDirection::default(),
    attach_public_key: false,
    sign: false,
    pgp_scheme: PgpScheme::default(),
    prompt_pin: false,
    sticky_labels: false,
    confirm_link: false,
    delay_send_seconds: 0,
    font_face: None,
    spam_action: None,
    block_sender_confirmation: None,
    mobile_settings: None,
    hide_remote_images: false,
    hide_embedded_images: false,
    hide_sender_images: false,
});
