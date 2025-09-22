use proton_core_api::services::proton::{
    Address as ApiAddress, DelinquentState, Flags as ApiFlags, Label as ApiLabel,
    ProductUsedSpace as ApiProductUsedSpace, User as ApiUser,
    UserMnemonicStatus as ApiUserMnemonicStatus, UserType as ApiUserType,
};
use proton_core_api::services::proton::{AddressId, LabelId, LabelType as ApiLabelType, UserId};
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_core_common::test_utils::addresses::ApiAddressTestUtils;
use proton_crypto_account::keys::{ArmoredPrivateKey, KeyId, LockedKey, UserKeys as ApiUserKeys};
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageBody as ApiMessageBody,
    MessageMetadata as ApiMessageMetadata, ViewMode as ApiViewMode,
};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::{ExclusiveLocation, SystemLabelId};
use proton_mail_common::models::{ConversationCounters, Message, MessageCounters};
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::params;
use stash::stash::{StashError, Tether};
use std::collections::HashMap;
use velcro::hash_map;

const TEST_USER_ID: &str =
    "jctxnoKsvmlISYpOtESCWNC4tcFbddXmcQ6yyM94YP4tBngrw4O9IKf8jxSLThqZyqFlX972kKwQCPriEeh4qg==";
const TEST_USER_ADDRESS_ID: &str =
    "LGXtB3TbNifsW1elXtCp5zyysma52yRf8NZZ10pUQrJfp1QQCSoFTXcIVDCZJycme6KYHsxCE_xdneJ10dt_iA==";

#[tokio::test]
async fn label_as_without_archive() {
    // Setup
    // * create 3 labels:
    //   + one for selected (1)
    //   + one for partially selected (2)
    //   + one for not selected (3)
    // * create 4 messages:
    //   + one without label
    //   + one with 2 + 3
    //   + one with 1 + 3
    //   + one with all three labels
    //
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();

    let label1_id = LabelId::from("selected");
    let label1 = test_label(&label1_id, "selected");
    let label2_id = LabelId::from("partial");
    let label2 = test_label(&label2_id, "partial");
    let label3_id = LabelId::from("unselected");
    let label3 = test_label(&label3_id, "unselected");

    let message1 = test_message("first", vec![]);
    let message2 = test_message("second", vec![label2_id.clone(), label3_id.clone()]);
    let message3 = test_message("third", vec![label1_id.clone(), label3_id.clone()]);
    let message4 = test_message(
        "fourth",
        vec![label1_id.clone(), label2_id.clone(), label3_id.clone()],
    );
    let labels = hash_map! {
        ApiLabelType::Label: vec![label1, label2, label3],
    };
    let params = test_init_params(labels);
    ctx.setup_user(params.clone()).await;

    ctx.mock_get_messages()
        .respond_with(vec![
            message1.metadata.clone(),
            message2.metadata.clone(),
            message3.metadata.clone(),
            message4.metadata.clone(),
        ])
        .await;

    ctx.mock_label_messages(
        &label1_id,
        vec![message1.metadata.id.clone(), message2.metadata.id.clone()],
    )
    .await;
    ctx.mock_unlabel_messages(
        &label3_id,
        vec![
            message2.metadata.id.clone(),
            message3.metadata.id.clone(),
            message4.metadata.id.clone(),
        ],
        vec![],
    )
    .await;
    ctx.catch_all().await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

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

    let (label1, label2) = tether
        .tx::<_, _, StashError>(async |tx| {
            let label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut msg_counters1 = MessageCounters::new(label1.id());
            msg_counters1.total = 2;
            msg_counters1.save(tx).await.unwrap();
            let mut conv_counters1 = ConversationCounters::new(label1.id());
            conv_counters1.total = 1;
            conv_counters1.save(tx).await.unwrap();
            let label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut msg_counters2 = MessageCounters::new(label2.id());
            msg_counters2.total = 2;
            msg_counters2.save(tx).await.unwrap();
            let mut conv_counters2 = ConversationCounters::new(label2.id());
            conv_counters2.total = 1;
            conv_counters2.save(tx).await.unwrap();
            let label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut msg_counters3 = MessageCounters::new(label3.id());
            msg_counters3.total = 3;
            msg_counters3.save(tx).await.unwrap();
            let mut conv_counters3 = ConversationCounters::new(label3.id());
            conv_counters3.total = 1;
            conv_counters3.save(tx).await.unwrap();
            Ok((label1, label2))
        })
        .await
        .unwrap();
    let message1 = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(message1.label_ids.is_empty());
    assert!(message1.custom_labels.is_empty());
    let message2 = Message::load(2.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message2.label_ids.len(), 2);
    assert_eq!(message2.custom_labels.len(), 2);
    let message3 = Message::load(3.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message3.label_ids.len(), 2);
    assert_eq!(message3.custom_labels.len(), 2);
    let message4 = Message::load(4.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message4.label_ids.len(), 3);
    assert_eq!(message4.custom_labels.len(), 3);

    // Action:
    let action_result = Message::action_label_as(
        &tether,
        user_ctx.action_queue(),
        inbox.id(),
        vec![message1.id(), message2.id(), message3.id(), message4.id()],
        vec![label1.id()],
        vec![label2.id()],
        false,
    )
    .await
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    // Validation:
    //   * All messages are in first label (=> 4)
    //   * All messages with second label still have it (=> 2)
    //   * No message have third label (=> 0)
    assert!(action_result.input_label_is_empty);
    let label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], &tether)
        .await
        .unwrap()
        .unwrap();
    let msg_counter1 = msg_counter_for(&label1, &tether).await;
    assert_eq!(msg_counter1.total, 4);
    let label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], &tether)
        .await
        .unwrap()
        .unwrap();
    let msg_counter2 = msg_counter_for(&label2, &tether).await;
    assert_eq!(msg_counter2.total, 2);
    let label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], &tether)
        .await
        .unwrap()
        .unwrap();
    let msg_counter3 = msg_counter_for(&label3, &tether).await;
    assert_eq!(msg_counter3.total, 0);
    let message1 = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message1.label_ids.len(), 1);
    assert!(message1.label_ids.contains(&label1_id));
    assert_eq!(message1.custom_labels.len(), 1);
    let message2 = Message::load(2.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message2.label_ids.len(), 2);
    assert!(message2.label_ids.contains(&label1_id));
    assert!(message2.label_ids.contains(&label2_id));
    assert_eq!(message2.custom_labels.len(), 2);
    let message3 = Message::load(3.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message3.label_ids.len(), 1);
    assert!(message3.label_ids.contains(&label1_id));
    assert_eq!(message3.custom_labels.len(), 1);
    let message4 = Message::load(4.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message4.label_ids.len(), 2);
    assert!(message4.label_ids.contains(&label1_id));
    assert!(message4.label_ids.contains(&label2_id));
    assert_eq!(message4.custom_labels.len(), 2);
}

#[tokio::test]
async fn label_as_with_archive() {
    // Setup
    // * create 3 labels:
    //   + one for selected (1)
    //   + one for partially selected (2)
    //   + one for not selected (3)
    // * create 2 messages:
    //   + one without label
    //   + one with all three labels
    //
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();

    let label1_id = LabelId::from("selected");
    let label1 = test_label(&label1_id, "selected");
    let label2_id = LabelId::from("partial");
    let label2 = test_label(&label2_id, "partial");
    let label3_id = LabelId::from("unselected");
    let label3 = test_label(&label3_id, "unselected");

    let message1 = test_message("first", vec![LabelId::inbox()]);
    let message2 = test_message(
        "second",
        vec![
            LabelId::inbox(),
            label1_id.clone(),
            label2_id.clone(),
            label3_id.clone(),
        ],
    );
    let labels = hash_map! {
        ApiLabelType::Label: vec![label1, label2, label3],
    };
    let params = test_init_params(labels);
    ctx.setup_user(params.clone()).await;

    ctx.mock_get_messages()
        .respond_with(vec![message1.metadata.clone(), message2.metadata.clone()])
        .await;

    ctx.mock_label_messages(&label1_id, vec![message1.metadata.id.clone()])
        .await;

    ctx.mock_unlabel_messages(&label3_id, vec![message2.metadata.id.clone()], vec![])
        .await;

    ctx.mock_label_messages(
        &LabelId::archive(),
        vec![message1.metadata.id.clone(), message2.metadata.id.clone()],
    )
    .await;

    ctx.catch_all().await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

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

    let (label1, label2) = tether
        .tx::<_, _, StashError>(async |tx| {
            let label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut msg_counters1 = MessageCounters::new(label1.id());
            msg_counters1.total = 1;
            msg_counters1.save(tx).await.unwrap();
            let mut conv_counters1 = ConversationCounters::new(label1.id());
            conv_counters1.total = 1;
            conv_counters1.save(tx).await.unwrap();
            let label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut msg_counters2 = MessageCounters::new(label2.id());
            msg_counters2.total = 1;
            msg_counters2.save(tx).await.unwrap();
            let mut conv_counters2 = ConversationCounters::new(label2.id());
            conv_counters2.total = 1;
            conv_counters2.save(tx).await.unwrap();
            let label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut msg_counters3 = MessageCounters::new(label3.id());
            msg_counters3.total = 1;
            msg_counters3.save(tx).await.unwrap();
            let mut conv_counters3 = ConversationCounters::new(label3.id());
            conv_counters3.total = 1;
            conv_counters3.save(tx).await.unwrap();
            Ok((label1, label2))
        })
        .await
        .unwrap();

    let message1 = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message1.label_ids.len(), 1);
    assert!(message1.custom_labels.is_empty());
    let message2 = Message::load(2.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message2.label_ids.len(), 4);
    assert_eq!(message2.custom_labels.len(), 3);

    // Action:
    let action_result = Message::action_label_as(
        &tether,
        user_ctx.action_queue(),
        inbox.id(),
        vec![message1.id(), message2.id()],
        vec![label1.id()],
        vec![label2.id()],
        true,
    )
    .await
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    // Validation:
    let archive_id = Label::remote_id_counterpart(LabelId::archive(), &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(action_result.input_label_is_empty);
    let message1 = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message1.label_ids.len(), 2);
    assert!(message1.label_ids.contains(&label1_id));
    assert!(message1.label_ids.contains(&LabelId::archive()));
    assert_eq!(message1.custom_labels.len(), 1);
    assert_eq!(
        message1.exclusive_location,
        Some(ExclusiveLocation::System {
            name: SystemLabel::Archive,
            local_id: archive_id,
        })
    );
    let message2 = Message::load(2.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message2.label_ids.len(), 3);
    assert!(message2.label_ids.contains(&label1_id));
    assert!(message2.label_ids.contains(&label2_id));
    assert!(message2.label_ids.contains(&LabelId::archive()));
    assert_eq!(message2.custom_labels.len(), 2);
    assert_eq!(
        message2.exclusive_location,
        Some(ExclusiveLocation::System {
            name: SystemLabel::Archive,
            local_id: archive_id,
        })
    );
}

fn test_label(label_id: &LabelId, name: &str) -> ApiLabel {
    ApiLabel {
        id: label_id.clone(),
        label_type: ApiLabelType::Label,
        name: name.to_owned(),
        ..ApiLabel::test_default()
    }
}

fn test_message(id: &str, label_ids: Vec<LabelId>) -> ApiMessage {
    let metadata = ApiMessageMetadata {
        id: MessageId::from(id.to_owned()),
        conversation_id: ConversationId::from("0R5oYZX2jLkT9WYyNrGmdp6K1sYYDraeaE8FTeNSJZ7Znb1UPJqBfvx_Tqb4gyVnGUeiPo3o7vKolaUt6PmVuw==".to_owned()),
        address_id: AddressId::from(TEST_USER_ADDRESS_ID),
        label_ids,
        size: 333,
        subject: "A simple message".to_owned(),
        time: 1715863508,
        ..ApiMessageMetadata::test_default()
    };

    ApiMessage {
        body:ApiMessageBody{
            body: "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DGS71hsmM2EQSAQdAYdJSo4eHIE7InFrOSN3+7nIRKfkcsCAb7aPI86nI\ny2owI0FLuN3IlbCoKsFFXfSbnTff3IePkr7xmhQmUYrVk0h50kwkEVyHnyPI\nm2nyqZXA0sCKAbKKQlcvjlJbsyUpJvsIwHuggwrQ+7htDauT4/SB9hScyAPj\nICxCGfzOaXjcf1fqevOMDqIWaSEQpOcMw2ocGP4I8OKgylBfuy9DT0/RhJSe\nrDo2uhlYqs0xmUdlHWPvGKEy4TKlUk2JSAr9U4+5l4J5iIK9O/TVrU+Tf7Ot\nRdEFfN+ERJQmVqXcfSkoImVm7oi0QfNP3ExZ94vlFyBFch/Ox5Oco5wbetr3\nL7KPGWiEmLYDI/xeFNC4AO4FD+MVUHjIYqzS/GABxwJQ7pCC8WJXUHKS6ZNR\nNf8RGKGL1O2cbKWSuULb7HwWRGljWezyr5rPLKK7DaHX3wj2qmdQRcSzsKEu\nOLjlB6jppMjP2r/CZSqC+XbefwczOZxkLJQiw6ujB4etdiDFiM+QifJfrp6f\nhtf7JGwpxPa/IbiL5OlKy7NYYs6JXNYU\n=AVU2\n-----END PGP MESSAGE-----\n".to_owned(),
                ..ApiMessageBody::test_default()
        },
        metadata,
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
    LockedKey  {
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

async fn msg_counter_for(label: &Label, tx: &Tether) -> MessageCounters {
    MessageCounters::find_by_id(label.id(), tx)
        .await
        .expect("failed to load")
        .expect("value not found")
}
