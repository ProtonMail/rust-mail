use proton_api_core::services::proton::{
    Address as ApiAddress, DelinquentState, Flags as ApiFlags, Label as ApiLabel,
    ProductUsedSpace as ApiProductUsedSpace, User as ApiUser,
    UserMnemonicStatus as ApiUserMnemonicStatus, UserType as ApiUserType,
};
use proton_api_core::services::proton::{AddressId, LabelId, LabelType as ApiLabelType, UserId};
use proton_api_mail::services::proton::common::{ConversationId, MessageId};
use proton_api_mail::services::proton::response_data::{
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageBody as ApiMessageBody,
    MessageFlags as ApiMessageFlags, MessageMetadata as ApiMessageMetadata,
    MimeType as ApiMimeType, ViewMode as ApiViewMode,
};
use proton_core_common::models::Label;
use proton_core_test_utils::addresses::ApiAddressTestUtils;
use proton_crypto_account::keys::{ArmoredPrivateKey, KeyId, LockedKey, UserKeys as ApiUserKeys};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{ConversationCounters, Message, MessageCounters};
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::params;
use stash::stash::StashError;
use std::collections::HashMap;
use velcro::hash_map;

const TEST_USER_ID: &str =
    "jctxnoKsvmlISYpOtESCWNC4tcFbddXmcQ6yyM94YP4tBngrw4O9IKf8jxSLThqZyqFlX972kKwQCPriEeh4qg==";
const TEST_USER_ADDRESS_ID: &str =
    "LGXtB3TbNifsW1elXtCp5zyysma52yRf8NZZ10pUQrJfp1QQCSoFTXcIVDCZJycme6KYHsxCE_xdneJ10dt_iA==";

#[tokio::test]
async fn move_between_folders() {
    // Setup:
    // * create 2 folder labels
    // * create a message in one of those folders
    let ctx = MailTestContext::new().await;

    let source_label_id = LabelId::from("source");
    let source_label = test_label(&source_label_id, ApiLabelType::Folder, "source");
    let destination_label_id = LabelId::from("destination");
    let destination_label = test_label(&destination_label_id, ApiLabelType::Folder, "destination");
    let message = test_message(vec![source_label_id.clone()], false);
    let labels = hash_map! {
        ApiLabelType::Folder: vec![ source_label, destination_label ]
    };
    let params = test_init_params(labels);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.mock_label_messages(
        &destination_label_id.clone(),
        vec![message.metadata.id.clone()],
    )
    .await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();

    // Create a mailbox and sync.
    let mailbox =
        Mailbox::with_remote_id(&user_ctx.user_stash().connection(), source_label_id.clone())
            .await
            .unwrap();
    mailbox
        .sync(&mut user_ctx.user_stash().connection(), user_ctx.api(), 10)
        .await
        .unwrap();

    let source = Label::find_first("WHERE remote_id = ?", params!["source"], &tether)
        .await
        .unwrap()
        .unwrap();

    let mut source_conv = ConversationCounters::new(source.local_id.expect("Local ID"));
    source_conv.total = 1;

    let mut source_msg = MessageCounters::new(source.local_id.expect("Local ID"));
    source_msg.total = 1;
    tether
        .tx::<_, _, StashError>(async |tx| {
            source_conv.save(tx).await.unwrap();
            source_msg.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let destination = Label::find_first("WHERE remote_id = ?", params!["destination"], &tether)
        .await
        .unwrap()
        .unwrap();

    let message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(message.label_ids.contains(&source_label_id));

    // Action:
    // * move message in the other folder
    Message::action_move(
        user_ctx.action_queue(),
        source.local_id.unwrap(),
        destination.local_id.unwrap(),
        vec![message.local_id.unwrap()],
    )
    .await
    .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation:
    // * the message is in the second folder
    // * the message is not in the first folder
    let message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(message.label_ids.len(), 1);
    assert!(!message.label_ids.contains(&source_label_id));
    assert!(message.label_ids.contains(&destination_label_id));
}

#[tokio::test]
async fn move_from_label_does_not_unlabel() {
    // Setup:
    // * create 2 custom labels
    // * create a message in one of those label
    let ctx = MailTestContext::new().await;

    let source_label_id = LabelId::from("source");
    let source_label = test_label(&source_label_id, ApiLabelType::Label, "source");
    let destination_label_id = LabelId::from("destination");
    let destination_label = test_label(&destination_label_id, ApiLabelType::Label, "destination");
    let message = test_message(vec![source_label_id.clone()], true);
    let labels = hash_map! {
        ApiLabelType::Label: vec![ source_label, destination_label ]
    };
    let params = test_init_params(labels);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.mock_label_messages(
        &destination_label_id.clone(),
        vec![message.metadata.id.clone()],
    )
    .await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(&user_ctx.user_stash().connection(), LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut user_ctx.user_stash().connection(), user_ctx.api(), 10)
        .await
        .unwrap();

    let source = Label::find_first("WHERE remote_id = ?", params!["source"], &tether)
        .await
        .unwrap()
        .unwrap();

    let destination = Label::find_first("WHERE remote_id = ?", params!["destination"], &tether)
        .await
        .unwrap()
        .unwrap();

    let message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(message.label_ids.contains(&source_label_id));
    assert_eq!(message.custom_labels.len(), 1);
    assert_eq!(message.custom_labels[0].name, "source");

    // Action:
    // * move message in the other label
    Message::action_move(
        user_ctx.action_queue(),
        source.local_id.unwrap(),
        destination.local_id.unwrap(),
        vec![message.local_id.unwrap()],
    )
    .await
    .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation:
    // * the message is in the second label
    // * the message is still in the first label
    let message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(message.label_ids.len(), 2);
    assert!(message.label_ids.contains(&source_label_id));
    assert!(message.label_ids.contains(&destination_label_id));
}

#[tokio::test]
async fn move_into_trash_remove_label_and_mark_read() {
    // Setup:
    // * create a label
    // * create a message in inbox (or any non-trash mailbox)
    // * add the label to the message
    // * the message is unread
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();
    let trash = Label::find_first("WHERE remote_id = ?", params![LabelId::trash()], &tether)
        .await
        .unwrap()
        .unwrap();

    let custom_label_id = LabelId::from("custom");
    let custom_label = test_label(&custom_label_id, ApiLabelType::Label, "custom");
    let message = test_message(
        vec![
            custom_label_id.clone(),
            LabelId::inbox(),
            LabelId::all_mail(),
        ],
        true,
    );
    let labels = hash_map! {
        ApiLabelType::Label: vec![ custom_label ],
    };
    let params = test_init_params(labels);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.mock_label_messages(&trash.remote_id.unwrap(), vec![message.metadata.id.clone()])
        .await;
    ctx.catch_all().await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(&user_ctx.user_stash().connection(), LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut user_ctx.user_stash().connection(), user_ctx.api(), 10)
        .await
        .unwrap();

    let message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(message.label_ids.contains(&custom_label_id));
    assert!(message.unread);

    // Action:
    // * move message in trash
    Message::action_move(
        user_ctx.action_queue(),
        inbox.local_id.unwrap(),
        trash.local_id.unwrap(),
        vec![message.local_id.unwrap()],
    )
    .await
    .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation:
    // * the message only have `all_mail` label
    // * the message is marked as read
    let message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert!(!message.label_ids.contains(&custom_label_id));
    assert!(message.label_ids.contains(&LabelId::all_mail()));
    assert!(!message.unread);
}

#[tokio::test]
async fn move_into_spam_remove_labels() {
    // Setup:
    // * create a label
    // * create a message in inbox (or any non-spam mailbox)
    // * add the label to the message
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    let spam = Label::find_first("WHERE remote_id = ?", params![LabelId::spam()], &tether)
        .await
        .unwrap()
        .unwrap();

    let custom_label_id = LabelId::from("custom");
    let custom_label = test_label(&custom_label_id, ApiLabelType::Label, "custom");
    let message = test_message(
        vec![
            custom_label_id.clone(),
            LabelId::inbox(),
            LabelId::all_mail(),
        ],
        false,
    );
    let labels = hash_map! {
        ApiLabelType::Label: vec![ custom_label ],
    };
    let params = test_init_params(labels);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.mock_label_messages(&spam.remote_id.unwrap(), vec![message.metadata.id.clone()])
        .await;
    ctx.catch_all().await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(&user_ctx.user_stash().connection(), LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut user_ctx.user_stash().connection(), user_ctx.api(), 10)
        .await
        .unwrap();

    let custom = Label::find_first("WHERE remote_id = ?", params!["custom"], &tether)
        .await
        .unwrap()
        .unwrap();

    let message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(message.label_ids.contains(&custom_label_id));

    // Action:
    // * move message in spam
    Message::action_move(
        user_ctx.action_queue(),
        custom.local_id.unwrap(),
        spam.local_id.unwrap(),
        vec![message.local_id.unwrap()],
    )
    .await
    .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation:
    // * the message only have `all_mail` label
    let message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert!(!message.label_ids.contains(&custom_label_id));
    assert!(message.label_ids.contains(&LabelId::all_mail()));
}

#[tokio::test]
async fn move_out_of_spam_set_almost_all_mail() {
    // Setup:
    // * create a message in spam
    // * the message doesn't have `almost_all_mail` label
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();

    let spam = Label::find_first("WHERE remote_id = ?", params![LabelId::spam()], &tether)
        .await
        .unwrap()
        .unwrap();

    let mut spam_conv = ConversationCounters::new(spam.local_id.expect("Local ID"));
    spam_conv.total = 1;

    let mut spam_msg = MessageCounters::new(spam.local_id.expect("Local ID"));
    spam_msg.total = 1;
    tether
        .tx::<_, _, StashError>(async |tx| {
            spam_conv.save(tx).await.unwrap();
            spam_msg.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();

    let message = test_message(vec![LabelId::spam()], false);
    let params = test_init_params(HashMap::new());
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.mock_label_messages(&inbox.remote_id.unwrap(), vec![message.metadata.id.clone()])
        .await;
    ctx.catch_all().await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(&user_ctx.user_stash().connection(), LabelId::spam())
        .await
        .unwrap();
    mailbox
        .sync(&mut user_ctx.user_stash().connection(), user_ctx.api(), 10)
        .await
        .unwrap();

    let message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message.label_ids.len(), 1);
    assert_eq!(message.label_ids[0].as_str(), "4");

    // Action:
    // * move message out of spam
    Message::action_move(
        user_ctx.action_queue(),
        spam.local_id.unwrap(),
        inbox.local_id.unwrap(),
        vec![message.local_id.unwrap()],
    )
    .await
    .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation:
    // * the message have `almost_all_mail` label
    let message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(message.label_ids.len(), 2);
    assert!(message.label_ids.contains(&LabelId::inbox()));
    assert!(message.label_ids.contains(&LabelId::almost_all_mail()));
}

fn test_label(label_id: &LabelId, label_type: ApiLabelType, name: &str) -> ApiLabel {
    ApiLabel {
        id: label_id.clone(),
        parent_id: None,
        name: name.to_owned(),
        path: None,
        color: String::new(),
        label_type,
        notify: false,
        display: false,
        sticky: false,
        expanded: false,
        order: 0,
    }
}

fn test_message(label_ids: Vec<LabelId>, unread: bool) -> ApiMessage {
    ApiMessage {
        metadata: ApiMessageMetadata {
            id: MessageId::from("blkMQzCHplN2H_FNJ2GdMtRkmr3f9v_cFma64_Cmi8IPw3wx_lK-0ZEqA8cBfIf0PeVbY2P7oVQVwPup-h0syg==".to_owned()),
            conversation_id: ConversationId::from("0R5oYZX2jLkT9WYyNrGmdp6K1sYYDraeaE8FTeNSJZ7Znb1UPJqBfvx_Tqb4gyVnGUeiPo3o7vKolaUt6PmVuw==".to_owned()),
            order: 0,
            address_id: AddressId::from(TEST_USER_ADDRESS_ID),
            label_ids,
            external_id: None,
            subject: "A simple message".to_owned(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            reply_tos: vec![],
            flags: ApiMessageFlags::DKIM_FAIL,
            time: 1715863508,
            size: 333,
            unread,
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
            mime_type: ApiMimeType::TextHtml,
            attachments: vec![],
        },
    }
}

fn test_init_params(labels: HashMap<ApiLabelType, Vec<ApiLabel>>) -> TestParams {
    TestParams {
        user_info: Some(test_user_info()),
        addresses: ApiAddress::test_addresses(),
        mail_settings: Some(test_mail_settings()),
        labels,
        ..Default::default()
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
        role: 0,
        private: 0,
        subscribed: 0,
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

fn test_mail_settings() -> ApiMailSettings {
    ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    }
}
