#![allow(non_snake_case)]

use crate::account::{testdata_user_secret, TEST_USER_ID, TEST_USER_MAIL};
use lazy_static::lazy_static;
use proton_api_core::auth::{AuthSession, AuthState, SecretString, UserKeySecret};
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_api_core::services::proton::Config;
use proton_api_mail::services::proton::response_data::{
    AttachmentMetadata as ApiAttachmentMetadata, Conversation as ApiConversation,
    ConversationLabel as ApiConversationLabel, MessageAddress as ApiMessageAddress,
};
use proton_api_mail::services::proton::response_data::{Label as ApiLabel, MessageMetadata};
use proton_core_common::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, Id, LabelId, LocalId,
    PasswordMode, RemoteId, TfaStatus,
};
use proton_core_common::db::account::{CoreAccount, CoreSession, SessionEncryptionKey};
use proton_core_common::models::{Address, ModelExtension};
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_crypto_account::keys::AddressKeys as RealAddressKeys;
use proton_mail_common::datatypes::{LabelColor, LabelType, SystemLabelId};
use proton_mail_common::models::Label;
use proton_mail_common::{MailContext, MailUserContext};
pub use secrecy::{ExposeSecret, SecretString as RealSecretString};
use stash::orm::Model;
use stash::stash::Stash;
use stash::stash::{Interface, Tether};
use std::sync::Arc;
use tempdir::TempDir;
use url::Url;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request};

lazy_static! {
    pub static ref MY_ADDRESS_ID: ApiRemoteId = ApiRemoteId::from("MyRemoteId");
    pub static ref MY_LABEL_ID1: ApiRemoteId = ApiRemoteId::from("MyLabelID1");
    pub static ref MY_LABEL_ID2: ApiRemoteId = ApiRemoteId::from("MyLabelID2");
    pub static ref MY_ATTACHMENT_ID: ApiRemoteId = ApiRemoteId::from("MyAttachmentID1");
    pub static ref MY_CONVERSATION_ID: ApiRemoteId = ApiRemoteId::from("MyConversationID");
}

/// Macro wrapping u64 into Option<LocalId> for easier model definition.
#[macro_export]
macro_rules! lid {
    ($id:expr) => {{
        use proton_core_common::datatypes::LocalId;
        Some(LocalId::from($id))
    }};
}

/// Macro wrapping &str into Option<RemoteId> for easier model definition.
/// Since it calls .into() on the RemoteId, it allows creation of Option<LabelId> as well.
#[macro_export]
macro_rules! rid {
    ($id:expr) => {{
        use proton_core_common::datatypes::RemoteId;
        Some(RemoteId::from($id).into())
    }};
}

#[macro_export]
macro_rules! label {
    ($($field:tt)*) => {{
        use proton_mail_common::models::Label;

        Label {
            $($field)*,
            ..Default::default()
        }}
    };
}

#[macro_export]
macro_rules! api_label {
    ($($field:tt)*) => {{
        use proton_api_mail::services::proton::response_data::{Label as ApiLabel};

        ApiLabel {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! message {
    ($($field:tt)*) => {{
        use proton_mail_common::models::Message;

        Message {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! api_message {
    ($($field:tt)*) => {{
        use proton_api_mail::services::proton::response_data::{Message as ApiMessage};

        ApiMessage {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! api_message_meta {
    ($($field:tt)*) => {{
        use proton_api_mail::services::proton::response_data::{MessageMetadata as ApiMessageMetadata};

        ApiMessageMetadata {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! conversation {
    ($($field:tt)*) => {{
        use proton_mail_common::models::Conversation;

        Conversation {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! api_conversation {
    ($($field:tt)*) => {{
        use proton_api_mail::services::proton::response_data::{Conversation as ApiConversation};

        ApiConversation {
            $($field)*,
            ..Default::default()
        }
    }};
}

/// Test context for mail tests.
///
/// This struct provides a test context with a handcrafted new session, so that
/// we can bypass authentication. It also spins up a mock server.
///
#[expect(dead_code)]
pub struct TestContext {
    context: MailContext,
    mock_server: MockServer,
    tmp_dir: TempDir,
    core_account: CoreAccount,
    core_session: CoreSession,
}

impl TestContext {
    /// Generate a test UID.
    fn test_uid() -> RemoteId {
        RemoteId::from("TEST_UID")
    }

    /// Generate a test user ID.
    fn test_user_id() -> RemoteId {
        RemoteId::from(TEST_USER_ID)
    }

    /// Generate a test user name or address.
    fn test_user_mail() -> String {
        TEST_USER_MAIL.to_owned()
    }

    /// Generate a test access token.
    fn test_acctok() -> SecretString {
        SecretString::from("ACCESSTOKEN".to_owned())
    }

    /// Generate a test refresh token.
    fn test_reftok() -> SecretString {
        SecretString::from("REFRESHTOKEN".to_owned())
    }

    /// Generate test scopes.
    fn test_scopes() -> Vec<String> {
        vec!["foo".to_owned(), "bar".to_owned()]
    }

    /// Create and initialize test context.
    pub async fn new() -> Self {
        Self::_new(None, None).await
    }

    /// Create and initialize test context and override the default `user_key_secret` and `user_id`.
    pub async fn with_user_secret_and_user_id(
        user_key_secret: UserKeySecret,
        user_id: RemoteId,
    ) -> Self {
        Self::_new(Some(user_key_secret), Some(user_id)).await
    }

    async fn _new(user_key_secret: Option<UserKeySecret>, user_id: Option<RemoteId>) -> Self {
        let mock_server = MockServer::start().await;

        // Create client with the mock server as the base URL
        let api_config = Config {
            base_url: format!("{}/api/", mock_server.uri()),
            allow_http: true,
            skip_srp_proof_validation: true,
            ..Default::default()
        };
        _ = Url::parse(&api_config.base_url).expect("Invalid URL");

        // Use the given data or fall back to the default
        let user_id = user_id.unwrap_or_else(Self::test_user_id);
        let user_key_secret = user_key_secret.unwrap_or_else(testdata_user_secret);

        // Create a temporary directory for the database
        let tmp_dir = TempDir::new("pmc_test").expect("failed to create temp dir");
        let keychain = Arc::new(InMemoryKeyChain::default());

        let cache_path = tmp_dir.path().join("mail-cache");
        std::fs::create_dir_all(&cache_path).expect("failed to create mail cache dir");

        // Generate a random encryption key and store it in the keychain
        let encryption_key = SessionEncryptionKey::random();
        keychain
            .store(encryption_key.to_base64())
            .expect("failed to store in keychain");

        // Create mail context
        let context = MailContext::new(
            tmp_dir.path(),
            tmp_dir.path(),
            cache_path,
            100_000, // ~100kB
            keychain,
            api_config,
            None,
        )
        .await
        .expect("failed to create mail context");

        // Generate a fake session and write it to the database
        let (core_account, core_session) = {
            // Create a temporary stash just to insert the fake data.
            let path = tmp_dir.path().join("account.db");
            let stash = Stash::new(Some(&path)).expect("failed to create stash");

            // Create a fake account.
            let account = CoreAccount::new(
                user_id.clone(),
                Self::test_user_mail(),
                TfaStatus::None,
                PasswordMode::One,
            )
            .with_stash(&stash)
            .with_save()
            .await
            .expect("fake account should save");

            // Create a auth session.
            let auth = AuthSession {
                uid: Self::test_uid().into(),
                name_or_addr: Self::test_user_mail(),
                user_id: user_id.clone().into(),
                second_factor_mode: TfaStatus::None.into(),
                password_mode: PasswordMode::One.into(),
                access_token: Self::test_acctok(),
                refresh_token: Self::test_reftok(),
                auth_scope: Self::test_scopes(),
                auth_state: AuthState::Ready,
            };

            // Create a fake session.
            let session = CoreSession::new(auth, &encryption_key)
                .expect("session should be created")
                .with_key_secret(&user_key_secret, &encryption_key)
                .expect("key secret should be set")
                .with_stash(&stash)
                .with_save()
                .await
                .expect("fake session should save");

            (account, session)
        };

        Self {
            mock_server,
            context,
            tmp_dir,
            core_account,
            core_session,
        }
    }

    /// Get the mail context.
    pub fn context(&self) -> &MailContext {
        &self.context
    }

    /// Get the Wiremock server.
    pub fn mock_server(&self) -> &MockServer {
        &self.mock_server
    }

    /// Set up a catch-all mock for the mock server.
    ///
    /// Calls to this function need to come at the END of the test setup, AFTER
    /// all other mocks have been set up. This will ensure that any unconfigured
    /// calls will cause the test to fail.
    ///
    /// It is unfortunately not possible to use the [`Mock::with_priority()`]
    /// method to set this up by default as a lower-priority expectation and
    /// establish a catch-all in that way.
    ///
    pub async fn catch_all(&self) {
        // If there are any unconfigured calls, we will panic because it's not what
        // we expect to happen, so the test should fail
        Mock::given(any())
            .respond_with(|request: &Request| {
                panic!(
                    "Received unexpected {} request\n  Path: {}\n  Headers:\n{}\n  Body: {}\n",
                    request.method,
                    request.url.path(),
                    request
                        .headers
                        .iter()
                        .map(|header| format!("    {}: {:?}", header.0, header.1))
                        .collect::<Vec<String>>()
                        .join("\n"),
                    String::from_utf8(request.body.clone()).unwrap(),
                );
            })
            .mount(&self.mock_server)
            .await;
    }

    /// Get the test user mail context.
    pub async fn user_context(&self) -> Arc<MailUserContext> {
        self.context
            .user_context_from_session(&self.core_session)
            .await
            .expect("failed to create user context")
    }

    pub fn get_test_labels(&self) -> Vec<ApiLabel> {
        let label1 = ApiLabel {
            id: "Label1".into(),
            name: "Label1".into(),
            ..Default::default()
        };
        let label2 = ApiLabel {
            id: "Label2".into(),
            name: "Label2".into(),
            ..Default::default()
        };
        let label3 = ApiLabel {
            id: "Label3".into(),
            name: "Label3".into(),
            ..Default::default()
        };

        vec![label1, label2, label3]
    }

    pub fn default_address(&self) -> ApiAddress {
        ApiAddress {
            id: "".into(),
            address_type: ApiAddressType::Original,
            catch_all: Default::default(),
            display_name: Default::default(),
            domain_id: Default::default(),
            email: Default::default(),
            keys: RealAddressKeys(vec![]),
            order: Default::default(),
            proton_mx: Default::default(),
            receive: Default::default(),
            send: Default::default(),
            signature: Default::default(),
            signed_key_list: Default::default(),
            status: ApiAddressStatus::Enabled,
        }
    }

    pub fn get_test_addrs(&self) -> Vec<ApiAddress> {
        let addr1 = ApiAddress {
            id: "Addr1".into(),
            email: "foo@bar".into(),
            ..self.default_address()
        };
        let addr2 = ApiAddress {
            id: "Addr2".into(),
            email: "foo@baz".into(),
            ..self.default_address()
        };

        vec![addr1, addr2]
    }

    pub fn default_conv_label(&self) -> ApiConversationLabel {
        ApiConversationLabel {
            id: "".into(),
            context_expiration_time: Default::default(),
            context_num_attachments: Default::default(),
            context_num_messages: Default::default(),
            context_num_unread: Default::default(),
            context_size: Default::default(),
            context_snooze_time: Default::default(),
            context_time: Default::default(),
        }
    }

    pub fn get_test_convers(&self) -> Vec<ApiConversation> {
        vec![ApiConversation {
            id: "Conv1".into(),
            labels: vec![
                ApiConversationLabel {
                    id: "Label1".into(),
                    ..self.default_conv_label()
                },
                ApiConversationLabel {
                    id: "Label2".into(),
                    ..self.default_conv_label()
                },
                ApiConversationLabel {
                    id: "Label3".into(),
                    ..self.default_conv_label()
                },
            ],
            ..Default::default()
        }]
    }

    pub fn get_test_msgs(&self) -> Vec<MessageMetadata> {
        let m1 = MessageMetadata {
            id: "Message1".into(),
            address_id: ApiRemoteId::from("Addr1"),
            label_ids: vec![ApiRemoteId::from("Label1"), ApiRemoteId::from("Label2")],
            ..Default::default()
        };
        let m2 = MessageMetadata {
            id: "Message2".into(),
            address_id: ApiRemoteId::from("Addr2"),
            label_ids: vec![ApiRemoteId::from("Label2"), ApiRemoteId::from("Label3")],
            ..Default::default()
        };
        vec![m1, m2]
    }
}

pub async fn remote_counterpart<T: Model>(id: LocalId, tx: &Tether) -> RemoteId {
    id.counterpart::<T, _>(tx).await.unwrap().unwrap()
}

#[allow(dead_code)]
pub async fn local_counterpart<T: Model>(id: RemoteId, tx: &Tether) -> LocalId {
    id.counterpart::<T, _>(tx).await.unwrap().unwrap()
}

pub async fn create_labels(tx: &Tether) -> Vec<LocalId> {
    let mut labels = [test_label1(), test_label2()];
    for label in &mut labels {
        label.save_using(tx).await.expect("failed to create labels");
        assert!(
            Label::find_by_id(RemoteId::from(label.remote_id.clone().unwrap()), tx.stash())
                .await
                .expect("failed to resolve label ids")
                .unwrap()
                .local_id
                .is_some()
        );
    }
    labels.into_iter().map(|l| l.local_id.unwrap()).collect()
}

pub async fn create_address(core_tx: &Tether) -> Address {
    let mut address = test_address();
    address
        .save_using(core_tx)
        .await
        .expect("failed to create address");

    address
}

pub fn test_address() -> Address {
    Address {
        local_id: None,
        remote_id: Some(MY_ADDRESS_ID.clone().into()),
        email: "hello@world".to_owned(),
        send: Default::default(),
        receive: Default::default(),
        status: AddressStatus::Enabled,
        domain_id: None,
        address_type: AddressType::Original,
        display_order: 0,
        display_name: "HelloWorld".to_owned(),
        signature: "SIGNATURE".to_owned(),
        keys: AddressKeys::default(),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList {
            min_epoch_id: None,
            max_epoch_id: None,
            expected_min_epoch_id: None,
            data: None,
            obsolescence_token: None,
            signature: None,
            revision: 0,
        },
        row_id: None,
        stash: None,
    }
}

pub fn test_label1() -> Label {
    label!(
        remote_id: Some(MY_LABEL_ID1.clone().into()),
        name: "MyLabel".to_owned(),
        color: LabelColor::black(),
        label_type: LabelType::Label
    )
}

pub fn test_label2() -> Label {
    label!(
       remote_id: Some(MY_LABEL_ID2.clone().into()),
       name: "MyFolder".to_owned(),
       color: LabelColor::black(),
       label_type: LabelType::Folder,
       notify: true,
       expanded: true,
       display_order: 1
    )
}

pub fn test_starred_label() -> Label {
    label!(
       remote_id: Some(LabelId::starred().clone()),
       name: "Starred".to_owned(),
       path: Some("Starred".to_owned()),
       color: LabelColor::black(),
       label_type: LabelType::System,
       display_order: 2
    )
}

pub fn test_conversation(
    labels: impl IntoIterator<Item = ApiConversationLabel>,
    attachments: impl IntoIterator<Item = ApiAttachmentMetadata>,
) -> ApiConversation {
    ApiConversation {
        id: MY_CONVERSATION_ID.clone(),
        order: 50,
        subject: "Hello World".to_owned(),
        senders: vec![ApiMessageAddress {
            address: "hello@world.com".to_owned(),
            name: "HelloWorld".to_owned(),
            ..Default::default()
        }],
        recipients: vec![
            ApiMessageAddress {
                address: "foo@bar.com".to_owned(),
                name: "Foo".to_owned(),
                ..Default::default()
            },
            ApiMessageAddress {
                address: "Bar@bar.com".to_owned(),
                name: "bar".to_owned(),
                ..Default::default()
            },
        ],
        num_messages: 10,
        num_unread: 4,
        num_attachments: 7,
        expiration_time: 1024,
        size: 4909,
        labels: Vec::from_iter(labels),
        display_snooze_reminder: false,
        attachments_metadata: Vec::from_iter(attachments),
        attachment_info: Default::default(),
    }
}
