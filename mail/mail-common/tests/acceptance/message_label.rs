use proton_core_api::services::proton::{
    Address as ApiAddress, DelinquentState, Flags as ApiFlags, Label as ApiLabel,
    ProductUsedSpace as ApiProductUsedSpace, Role as ApiRole, User as ApiUser,
    UserMnemonicStatus as ApiUserMnemonicStatus, UserType as ApiUserType,
};
use proton_core_api::services::proton::{AddressId, LabelId, LabelType as ApiLabelType, UserId};
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_core_common::test_utils::addresses::ApiAddressTestUtils;
use proton_crypto_account::keys::{ArmoredPrivateKey, KeyId, LockedKey, UserKeys as ApiUserKeys};
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{ConversationCount, MessageCount};
use proton_mail_api::services::proton::response_data::{
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageBody as ApiMessageBody,
    MessageFlags as ApiMessageFlags, MessageMetadata as ApiMessageMetadata,
    MimeType as ApiMimeType, ViewMode as ApiViewMode,
};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::MessageFlags;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Message;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::params;
use velcro::hash_map;

const TEST_USER_ID: &str =
    "jctxnoKsvmlISYpOtESCWNC4tcFbddXmcQ6yyM94YP4tBngrw4O9IKf8jxSLThqZyqFlX972kKwQCPriEeh4qg==";
const TEST_USER_ADDRESS_ID: &str =
    "LGXtB3TbNifsW1elXtCp5zyysma52yRf8NZZ10pUQrJfp1QQCSoFTXcIVDCZJycme6KYHsxCE_xdneJ10dt_iA==";

#[tokio::test]
async fn label_message() {
    // Setup:
    //  * Create a Label
    //  * Create a Message
    let ctx = MailTestContext::new().await;

    let label_id = LabelId::from("mylabel");
    let label = test_label(&label_id);
    let message = test_message();
    let params = test_init_params_label(label);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    ctx.mock_label_messages(&label_id.clone(), vec![message.metadata.id.clone()])
        .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();
    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();

    let label = Label::find_first("WHERE remote_id = ?", params!["mylabel"], &tether)
        .await
        .unwrap()
        .unwrap();
    let message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(message.custom_labels.is_empty());
    assert!(!message.label_ids.contains(&label_id));

    // Actions:
    //   * Apply the label to the message
    Message::action_apply_label(user_ctx.action_queue(), label.id(), vec![message.id()])
        .await
        .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Verification:
    //   * The message have the label
    let message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert!(message.label_ids.contains(&label_id));
    assert_eq!(message.custom_labels.len(), 1);
    assert_eq!(message.custom_labels[0].name, "mylabel");
}

#[tokio::test]
async fn unlabel_message() {
    // Setup:
    //  * Create a Label
    //  * Create a Message with this label
    let ctx = MailTestContext::new().await;

    let label_id = LabelId::from("mylabel");
    let label = test_label(&label_id);
    let message = test_message();
    let params = test_init_params_label(label);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    ctx.mock_label_messages(&label_id.clone(), vec![message.metadata.id.clone()])
        .await;

    ctx.mock_unlabel_messages(&label_id.clone(), vec![message.metadata.id.clone()], vec![])
        .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();
    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();

    let label = Label::find_first("WHERE remote_id = ?", params!["mylabel"], &tether)
        .await
        .unwrap()
        .unwrap();
    Message::action_apply_label(user_ctx.action_queue(), label.id(), vec![1.into()])
        .await
        .unwrap();

    user_ctx.execute_single_action().await.unwrap();

    let message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(message.label_ids.contains(&label_id));
    assert_eq!(message.custom_labels.len(), 1);
    assert_eq!(message.custom_labels[0].name, "mylabel");

    // Actions:
    //   * Apply the label to the message
    Message::action_remove_label(user_ctx.action_queue(), label.id(), vec![message.id()])
        .await
        .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Verification:
    //   * The message have the label
    let message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert!(message.custom_labels.is_empty());
    assert!(!message.label_ids.contains(&label_id));
}

#[tokio::test]
async fn message_action_read_unread() {
    // Setup:
    //  * Create a Label
    //  * Create a Message
    let ctx = MailTestContext::new().await;

    let label_id = LabelId::from("mylabel");
    let label = test_label(&label_id);
    let message = test_message();

    let params = test_init_params_label(label);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    ctx.mock_messages_ok().await;
    ctx.catch_all().await;

    let user_context = ctx.mail_user_context().await;
    let tether = user_context.user_stash().connection().await.unwrap();

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(
        &user_context.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();
    mailbox
        .sync(
            &mut user_context.user_stash().connection().await.unwrap(),
            user_context.session(),
            10,
        )
        .await
        .unwrap();

    let mut message = Message::load(1.into(), &tether).await.unwrap().unwrap();

    // This message starts as read
    assert!(!message.unread);

    // Actions:
    //   * Mark message unread
    Message::action_mark_unread(user_context.action_queue(), vec![message.id()])
        .await
        .unwrap();

    // Verification:
    //   * The message is unread
    message.reload(&tether).await.unwrap();
    assert!(message.unread);

    // Actions:
    //   * Mark message read
    Message::action_mark_read(user_context.action_queue(), vec![message.id()])
        .await
        .unwrap();

    // Verification:
    //   * The message is read
    message.reload(&tether).await.unwrap();
    assert!(!message.unread);
}

#[tokio::test]
async fn message_action_delete() {
    // Setup:
    //  * Create a Label
    //  * Create a Message
    let ctx = MailTestContext::new().await;

    let label_id = LabelId::from("mylabel");
    let label = test_label(&label_id);
    let message = test_message();
    let params = test_init_params_label(label);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    ctx.mock_messages_ok().await;
    ctx.catch_all().await;

    let user_context = ctx.mail_user_context().await;
    let tether = user_context.user_stash().connection().await.unwrap();

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(
        &user_context.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();
    mailbox
        .sync(
            &mut user_context.user_stash().connection().await.unwrap(),
            user_context.session(),
            10,
        )
        .await
        .unwrap();

    let label = Label::find_first("WHERE remote_id = ?", params!["mylabel"], &tether)
        .await
        .unwrap()
        .unwrap();
    let message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(!message.deleted);

    // Actions:
    //   * delete message
    Message::action_delete(user_context.action_queue(), label.id(), vec![message.id()])
        .await
        .unwrap();

    // Verification:
    //   * The message is marked as deleted
    let message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert!(message.deleted);
}

#[tokio::test]
async fn message_action_ham() {
    let ctx = MailTestContext::new().await;

    let mut message = test_message();
    message.metadata.label_ids = vec![LabelId::spam()];

    let params = TestParams {
        user_info: Some(test_user_info()),
        addresses: ApiAddress::test_addresses(),
        mail_settings: Some(test_mail_settings()),
        conversation_count: vec![ConversationCount {
            label_id: SystemLabel::Inbox.remote_id(),
            total: 1,
            unread: 0,
        }],
        message_count: vec![MessageCount {
            label_id: SystemLabel::Inbox.remote_id(),
            total: 1,
            unread: 0,
        }],
        ..Default::default()
    };

    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_label_messages(&LabelId::inbox(), vec![message.metadata.id.clone()])
        .await;

    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    ctx.mock_put_message_ham(&message.metadata.id).await;
    ctx.mock_empty_label().await;

    ctx.catch_all().await;

    let user_context = ctx.mail_user_context().await;
    let tether = user_context.user_stash().connection().await.unwrap();
    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(
        &user_context.user_stash().connection().await.unwrap(),
        LabelId::spam(),
    )
    .await
    .unwrap();
    mailbox
        .sync(
            &mut user_context.user_stash().connection().await.unwrap(),
            user_context.session(),
            10,
        )
        .await
        .unwrap();

    {
        // First: No messages in inbox
        let local_inbox = Label::resolve_local_label_id(LabelId::inbox(), &tether)
            .await
            .unwrap();

        let messages = Message::in_label(local_inbox, &tether).await.unwrap();
        assert_eq!(messages.len(), 0);

        // Only message is in spam
        let local_spam = Label::resolve_local_label_id(LabelId::spam(), &tether)
            .await
            .unwrap();

        let messages = Message::in_label(local_spam, &tether).await.unwrap();
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert!(!message.flags.contains(MessageFlags::HAM_MANUAL));

        // Mark it as ham
        Message::action_ham(user_context.action_queue(), vec![message.id()])
            .await
            .unwrap();
    }

    user_context.execute_all_actions().await.unwrap();

    let local_inbox = Label::remote_id_counterpart(LabelId::inbox(), &tether)
        .await
        .unwrap()
        .unwrap();
    {
        let messages = Message::in_label(local_inbox, &tether).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].flags.contains(MessageFlags::HAM_MANUAL));
    }

    Message::action_delete_all_in_label(user_context.action_queue(), local_inbox, &tether)
        .await
        .unwrap();

    user_context.execute_all_actions().await.unwrap();
    let messages = Message::find("WHERE deleted = 1", vec![], &tether)
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
}

fn test_init_params_label(label: ApiLabel) -> TestParams {
    let labels = hash_map! {
        ApiLabelType::Label: vec![label],
    };

    TestParams {
        user_info: Some(test_user_info()),
        addresses: ApiAddress::test_addresses(),
        mail_settings: Some(test_mail_settings()),
        labels,
        conversation_count: vec![ConversationCount {
            label_id: SystemLabel::Inbox.remote_id(),
            total: 1,
            unread: 0,
        }],
        message_count: vec![MessageCount {
            label_id: SystemLabel::Inbox.remote_id(),
            total: 1,
            unread: 0,
        }],
        ..Default::default()
    }
}

fn test_mail_settings() -> ApiMailSettings {
    ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    }
}

fn test_label(label_id: &LabelId) -> ApiLabel {
    ApiLabel {
        id: label_id.clone(),
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
    }
}

fn test_user_info() -> ApiUser {
    ApiUser {
        id: UserId::from(TEST_USER_ID),
        name: Some("rust_test".to_owned()),
        display_name: None,
        email: "rust_test@proton.ch".to_owned(),
        used_space: 0,
        max_space: 0,
        max_upload: 0,
        user_type: ApiUserType::Proton,
        create_time: 0,
        credit: 0,
        currency: "EUR".to_owned(),
        keys: ApiUserKeys(vec![test_user_key()]),
        product_used_space: ApiProductUsedSpace {
            calendar: 0,
            contact: 0,
            drive: 0,
            mail: 0,
            pass: 0,
        },
        to_migrate: false,
        mnemonic_status: ApiUserMnemonicStatus::Unknown,
        role: ApiRole::None,
        private: false,
        subscribed: 0,
        services: 0,
        delinquent: DelinquentState::NotReceived,
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
    }
}

fn test_user_key() -> LockedKey {
    LockedKey {
        id: KeyId::from("aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ=="),
        version: 3,
        private_key: ArmoredPrivateKey::from("-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZie3jRYJKwYBBAHaRw8BAQdAAp+4PE1Sf5V95XrIY/P2dUNk1TOojoEG\nLuuOzULTa1v+CQMINYn0u3DCV01gjT+Noe2HzLxwP2hieZC1aoGCxSrLn0fs\nLeShqv2pCPZ+SdrjXB5s5Rq7OP5Kr/2gN+0KS0yLGdyirFZWe6m5T8j20UQ5\n0M07bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp\nbF91c2VAZG9tYWluLnRsZD7CjAQQFgoAPgWCZie3jQQLCQcICZA4nKgbRZBl\nGQMVCAoEFgACAQIZAQKbAwIeARYhBOZJEArPLqrMMxX8fzicqBtFkGUZAADk\n/AD+LA6NW1K+Z3IT66/DEtjH0cmw6HNqxkBdT7kaL2o5pAMA/j9b4JCurWk/\n62MBM4I9RwXzSo8lmgPiYwPp4d/xgEsMx4sEZie3jRIKKwYBBAGXVQEFAQEH\nQHvLC7RWIDsorX5ZmYwjZbUhbXnEcO2sYt8OFaIh5KtHAwEIB/4JAwhKivkG\nshycUGA6wZtPR2HqO6+jvvSlRau/g2eZnWqhnvB4iIYTcD+CPpcPnWrrNgTz\nAU+kQ5sVrP6OiKKHIkUvHT5+MwelTbcpievGx2zGwngEGBYKACoFgmYnt40J\nkDicqBtFkGUZApsMFiEE5kkQCs8uqswzFfx/OJyoG0WQZRkAAJ6BAQDv4nBl\nNnj0W7XiAjiwRmVrY/sdybelB6j01p7UrcVAxQEAtEmT2cSIScVdWH1j3H9l\n0gGE7amH+cm6CjXOA7+Uwwc=\n=RGJ0\n-----END PGP PRIVATE KEY BLOCK-----\n".to_owned()),
        token: None,
        signature: None,
        activation: None,
        primary: true,
        active: true,
        flags: None,
        recovery_secret: None,
        recovery_secret_signature: None,
        address_forwarding_id: None,
    }
}

fn test_message() -> ApiMessage {
    ApiMessage {
        metadata: ApiMessageMetadata {
            id: MessageId::from("blkMQzCHplN2H_FNJ2GdMtRkmr3f9v_cFma64_Cmi8IPw3wx_lK-0ZEqA8cBfIf0PeVbY2P7oVQVwPup-h0syg==".to_owned()),
            conversation_id: ConversationId::from("0R5oYZX2jLkT9WYyNrGmdp6K1sYYDraeaE8FTeNSJZ7Znb1UPJqBfvx_Tqb4gyVnGUeiPo3o7vKolaUt6PmVuw==".to_owned()),
            order: 0,
            address_id: AddressId::from(TEST_USER_ADDRESS_ID),
            label_ids: vec![LabelId::inbox()],
            external_id: None,
            subject: "A simple message".to_owned(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            flags: ApiMessageFlags::DKIM_FAIL,
            time: 1715863508,
            size: 333,
            unread: false,
            is_replied: false,
            is_replied_all: false,
            is_forwarded: false,
            expiration_time: 0,
            snooze_time: 0,
            num_attachments: 0,
            attachments_metadata: vec![],
        },
        body: ApiMessageBody {
            header: String::new(),
            parsed_headers: Default::default(),
            body: "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DGS71hsmM2EQSAQdAYdJSo4eHIE7InFrOSN3+7nIRKfkcsCAb7aPI86nI\ny2owI0FLuN3IlbCoKsFFXfSbnTff3IePkr7xmhQmUYrVk0h50kwkEVyHnyPI\nm2nyqZXA0sCKAbKKQlcvjlJbsyUpJvsIwHuggwrQ+7htDauT4/SB9hScyAPj\nICxCGfzOaXjcf1fqevOMDqIWaSEQpOcMw2ocGP4I8OKgylBfuy9DT0/RhJSe\nrDo2uhlYqs0xmUdlHWPvGKEy4TKlUk2JSAr9U4+5l4J5iIK9O/TVrU+Tf7Ot\nRdEFfN+ERJQmVqXcfSkoImVm7oi0QfNP3ExZ94vlFyBFch/Ox5Oco5wbetr3\nL7KPGWiEmLYDI/xeFNC4AO4FD+MVUHjIYqzS/GABxwJQ7pCC8WJXUHKS6ZNR\nNf8RGKGL1O2cbKWSuULb7HwWRGljWezyr5rPLKK7DaHX3wj2qmdQRcSzsKEu\nOLjlB6jppMjP2r/CZSqC+XbefwczOZxkLJQiw6ujB4etdiDFiM+QifJfrp6f\nhtf7JGwpxPa/IbiL5OlKy7NYYs6JXNYU\n=AVU2\n-----END PGP MESSAGE-----\n".to_owned(),
            reply_to: Default::default(),
            mime_type: ApiMimeType::TextHtml,
            attachments: vec![],
            reply_tos: vec![],
        },
    }
}
