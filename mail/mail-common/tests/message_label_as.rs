use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType, Flags as ApiFlags,
    ProductUsedSpace as ApiProductUsedSpace, User as ApiUser,
    UserMnemonicStatus as ApiUserMnemonicStatus, UserType as ApiUserType,
};
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_api_mail::services::proton::response_data::{
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageBody as ApiMessageBody,
    MessageMetadata as ApiMessageMetadata, ViewMode as ApiViewMode,
};
use proton_core_common::datatypes::{Id, LabelId};
use proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, ArmoredPrivateKey, EncryptedKeyToken, KeyFlag, KeyId,
    KeyTokenSignature, LockedKey, UserKeys as ApiUserKeys,
};
use proton_mail_common::datatypes::{ExclusiveLocation, SystemLabel, SystemLabelId};
use proton_mail_common::models::{Label, Message};
use proton_mail_common::Mailbox;
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::test_context::MailTestContext;
use stash::orm::Model;
use stash::params;
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
    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();

    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], stash)
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
    let message2 = test_message(
        "second",
        vec![label2_id.clone().into(), label3_id.clone().into()],
    );
    let message3 = test_message(
        "third",
        vec![label1_id.clone().into(), label3_id.clone().into()],
    );
    let message4 = test_message(
        "fourth",
        vec![
            label1_id.clone().into(),
            label2_id.clone().into(),
            label3_id.clone().into(),
        ],
    );
    let labels = hash_map! {
        ApiLabelType::Label: vec![label1, label2, label3],
    };
    let params = test_init_params(labels);
    ctx.setup_user(params.clone()).await;

    ctx.mock_get_messages(vec![
        message1.metadata.clone(),
        message2.metadata.clone(),
        message3.metadata.clone(),
        message4.metadata.clone(),
    ])
    .await;
    ctx.mock_label_messages(
        &label1_id.clone().into_inner().into(),
        vec![message1.metadata.id.clone(), message2.metadata.id.clone()],
    )
    .await;
    ctx.mock_unlabel_messages(
        &label3_id.into_inner().into(),
        vec![
            message2.metadata.id.clone(),
            message3.metadata.id.clone(),
            message4.metadata.id.clone(),
        ],
        vec![],
    )
    .await;
    ctx.catch_all().await;

    ctx.init_user(user_ctx.clone()).await;

    let mailbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .unwrap();
    mailbox.sync(10).await.unwrap();

    let mut label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], stash)
        .await
        .unwrap()
        .unwrap();
    label1.total_msg = 2;
    label1.total_conv = 1;
    label1.save().await.unwrap();
    let mut label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], stash)
        .await
        .unwrap()
        .unwrap();
    label2.total_msg = 2;
    label2.total_conv = 1;
    label2.save().await.unwrap();
    let mut label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], stash)
        .await
        .unwrap()
        .unwrap();
    label3.total_msg = 3;
    label3.total_conv = 1;
    label3.save().await.unwrap();
    let message1 = Message::load(1.into(), stash).await.unwrap().unwrap();
    assert!(message1.label_ids.is_empty());
    assert!(message1.custom_labels.is_empty());
    let message2 = Message::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(message2.label_ids.len(), 2);
    assert_eq!(message2.custom_labels.len(), 2);
    let message3 = Message::load(3.into(), stash).await.unwrap().unwrap();
    assert_eq!(message3.label_ids.len(), 2);
    assert_eq!(message3.custom_labels.len(), 2);
    let message4 = Message::load(4.into(), stash).await.unwrap().unwrap();
    assert_eq!(message4.label_ids.len(), 3);
    assert_eq!(message4.custom_labels.len(), 3);

    // Action:
    let action_result = Message::action_label_as(
        user_ctx.queue(),
        inbox.local_id.unwrap(),
        vec![
            message1.local_id.unwrap(),
            message2.local_id.unwrap(),
            message3.local_id.unwrap(),
            message4.local_id.unwrap(),
        ],
        vec![label1.local_id.unwrap()],
        vec![label2.local_id.unwrap()],
        false,
    )
    .await
    .unwrap();

    // Validation:
    //   * All messages are in first label (=> 4)
    //   * All messages with second label still have it (=> 2)
    //   * No message have third label (=> 0)
    assert!(action_result);
    let label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label1.total_msg, 4);
    let label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label2.total_msg, 2);
    let label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label3.total_msg, 0);
    let message1 = Message::load(1.into(), stash).await.unwrap().unwrap();
    assert_eq!(message1.label_ids.len(), 1);
    assert!(message1.label_ids.contains(&label1_id));
    assert_eq!(message1.custom_labels.len(), 1);
    let message2 = Message::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(message2.label_ids.len(), 2);
    assert!(message2.label_ids.contains(&label1_id));
    assert!(message2.label_ids.contains(&label2_id));
    assert_eq!(message2.custom_labels.len(), 2);
    let message3 = Message::load(3.into(), stash).await.unwrap().unwrap();
    assert_eq!(message3.label_ids.len(), 1);
    assert!(message3.label_ids.contains(&label1_id));
    assert_eq!(message3.custom_labels.len(), 1);
    let message4 = Message::load(4.into(), stash).await.unwrap().unwrap();
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
    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();

    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], stash)
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
    let message2 = test_message(
        "second",
        vec![
            label1_id.clone().into(),
            label2_id.clone().into(),
            label3_id.clone().into(),
        ],
    );
    let labels = hash_map! {
        ApiLabelType::Label: vec![label1, label2, label3],
    };
    let params = test_init_params(labels);
    ctx.setup_user(params.clone()).await;

    ctx.mock_get_messages(vec![message1.metadata.clone(), message2.metadata.clone()])
        .await;
    ctx.mock_label_messages(
        &label1_id.clone().into_inner().into(),
        vec![message1.metadata.id.clone()],
    )
    .await;
    ctx.mock_unlabel_messages(
        &label3_id.into_inner().into(),
        vec![message2.metadata.id.clone()],
        vec![],
    )
    .await;
    ctx.mock_label_messages(
        &LabelId::archive().into(),
        vec![message1.metadata.id.clone(), message2.metadata.id.clone()],
    )
    .await;
    ctx.catch_all().await;

    ctx.init_user(user_ctx.clone()).await;

    let mailbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .unwrap();
    mailbox.sync(10).await.unwrap();

    let mut label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], stash)
        .await
        .unwrap()
        .unwrap();
    label1.total_msg = 1;
    label1.total_conv = 1;
    label1.save().await.unwrap();
    let mut label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], stash)
        .await
        .unwrap()
        .unwrap();
    label2.total_msg = 1;
    label2.total_conv = 1;
    label2.save().await.unwrap();
    let mut label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], stash)
        .await
        .unwrap()
        .unwrap();
    label3.total_msg = 1;
    label3.total_conv = 1;
    label3.save().await.unwrap();

    let message1 = Message::load(1.into(), stash).await.unwrap().unwrap();
    assert!(message1.label_ids.is_empty());
    assert!(message1.custom_labels.is_empty());
    let message2 = Message::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(message2.label_ids.len(), 3);
    assert_eq!(message2.custom_labels.len(), 3);

    // Action:
    let action_result = Message::action_label_as(
        user_ctx.queue(),
        inbox.local_id.unwrap(),
        vec![message1.local_id.unwrap(), message2.local_id.unwrap()],
        vec![label1.local_id.unwrap()],
        vec![label2.local_id.unwrap()],
        true,
    )
    .await
    .unwrap();

    // Validation:
    let archive_id = LabelId::archive()
        .counterpart::<Label, _>(stash)
        .await
        .unwrap()
        .unwrap();

    assert!(action_result);
    let message1 = Message::load(1.into(), stash).await.unwrap().unwrap();
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
    let message2 = Message::load(2.into(), stash).await.unwrap().unwrap();
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
        id: label_id.clone().into(),
        label_type: ApiLabelType::Label,
        name: name.to_owned(),
        ..Default::default()
    }
}

fn test_message(id: &str, label_ids: Vec<ApiRemoteId>) -> ApiMessage {
    let metadata = ApiMessageMetadata {
        id: ApiRemoteId::from(id.to_owned()),
        conversation_id: ApiRemoteId::from("0R5oYZX2jLkT9WYyNrGmdp6K1sYYDraeaE8FTeNSJZ7Znb1UPJqBfvx_Tqb4gyVnGUeiPo3o7vKolaUt6PmVuw==".to_owned()),
        address_id: ApiRemoteId::from(TEST_USER_ADDRESS_ID),
        label_ids,
        size: 333,
        subject: "A simple message".to_owned(),
        time: 1715863508,
        ..Default::default()
    };

    ApiMessage {
        body:ApiMessageBody{
            body: "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DGS71hsmM2EQSAQdAYdJSo4eHIE7InFrOSN3+7nIRKfkcsCAb7aPI86nI\ny2owI0FLuN3IlbCoKsFFXfSbnTff3IePkr7xmhQmUYrVk0h50kwkEVyHnyPI\nm2nyqZXA0sCKAbKKQlcvjlJbsyUpJvsIwHuggwrQ+7htDauT4/SB9hScyAPj\nICxCGfzOaXjcf1fqevOMDqIWaSEQpOcMw2ocGP4I8OKgylBfuy9DT0/RhJSe\nrDo2uhlYqs0xmUdlHWPvGKEy4TKlUk2JSAr9U4+5l4J5iIK9O/TVrU+Tf7Ot\nRdEFfN+ERJQmVqXcfSkoImVm7oi0QfNP3ExZ94vlFyBFch/Ox5Oco5wbetr3\nL7KPGWiEmLYDI/xeFNC4AO4FD+MVUHjIYqzS/GABxwJQ7pCC8WJXUHKS6ZNR\nNf8RGKGL1O2cbKWSuULb7HwWRGljWezyr5rPLKK7DaHX3wj2qmdQRcSzsKEu\nOLjlB6jppMjP2r/CZSqC+XbefwczOZxkLJQiw6ujB4etdiDFiM+QifJfrp6f\nhtf7JGwpxPa/IbiL5OlKy7NYYs6JXNYU\n=AVU2\n-----END PGP MESSAGE-----\n".to_owned(),
            ..Default::default()
        },
        metadata,
        ..Default::default()
    }
}

fn test_init_params(labels: HashMap<ApiLabelType, Vec<ApiLabel>>) -> TestParams {
    TestParams {
        user_info: Some(test_user_info()),
        addresses: test_addresses(),
        mail_settings: Some(test_mail_settings()),
        labels,
        ..Default::default()
    }
}

fn test_user_info() -> ApiUser {
    ApiUser {
        id: ApiRemoteId::from(TEST_USER_ID),
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
        delinquent: 0,
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

fn test_addresses() -> Vec<ApiAddress> {
    vec![ApiAddress {
        id: ApiRemoteId::from(TEST_USER_ADDRESS_ID),
        email: "rust_test@proton.ch".to_owned(),
        send: true,
        receive: true,
        status: ApiAddressStatus::Enabled,
        domain_id: None,
        address_type: ApiAddressType::Original,
        order: 0,
        display_name: "rust_test".to_owned(),
        signature: "".to_owned(),
        keys: ApiAddressKeys(
            vec![LockedKey{
                id: KeyId::from("gzKDANARz0i8OHhGuZV-oFfURju0I3XeW_hNn09g13dS_NJ57UbW420UAcWb-0s93xoav22O_jARq61FyL3guw=="),
                version: 3,
                private_key: ArmoredPrivateKey::from("-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZie3jRYJKwYBBAHaRw8BAQdA0lnAs/zJxwALYyLq9jnthTTJauaqwvLQ\nod3cCVOua+v+CQMIcWjkpeADcjxgwP+7tEc2sfM3J4oWV/p344AsSBiK442t\n5GmxcPBNuj7P82Mjfj10MfhzxIgDF39KW85vcrL4BRuDYq4uSUURFnZmiLFS\nx80vcnVzdF90ZXN0QHByb3Rvbi5ibGFjayA8cnVzdF90ZXN0QHByb3Rvbi5i\nbGFjaz7CjAQQFgoAPgWCZie3jQQLCQcICZDD5SnHczmG6wMVCAoEFgACAQIZ\nAQKbAwIeARYhBBGxOGij+OleubdsX8PlKcdzOYbrAABxyQEA53ij2BO8KHOi\nlmhaB9qeaNDnZhlvNazM9O87r2Cm03UA/jLgvtPQe+HgIDbguMFSeacvAKSG\n2A5jl6AAPWjifF4Jx4sEZie3jRIKKwYBBAGXVQEFAQEHQLJ401cWczKQigvx\njfQ5DxVXvA9p+HRuW16642Ybd99+AwEIB/4JAwjsnBN5czXnymCSAHHIugJH\nwwH1rvooZGeZ26QZ/UhsjQwXy1O5J66plmBD1Oe/uZG4Ed6ylw1VwROmW03q\nrRWwYeeVSN20YMavgbAZT7AVwngEGBYKACoFgmYnt40JkMPlKcdzOYbrApsM\nFiEEEbE4aKP46V65t2xfw+Upx3M5husAAPU7AQCMKF564vtdGCY/KIGqAhm2\nSNUnK5w6MkGKgrztbAhvngD/VK3t0WB8mUqXC3JoS2xC6rtyiyciAjQvuwWT\n2ePDxgI=\n=5IIS\n-----END PGP PRIVATE KEY BLOCK-----\n".to_owned()),
                token: Some(EncryptedKeyToken::from("-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DJ8rw1vR308gSAQdAwfey4aUSny0pDcCM0OykFF+KoquoUEuc5I48NYNn\nNkYwdMVXcHgrNAOVkSgBcCS5VxaRb3Lmo610XkQRnCyuadgvce4pRFqtx0+A\nNCNgn/Px0nEB+tPsQJL+EePQHgMZXhXmW3tS6/7jxzyCkuJVKdXHFNu3kTNU\nthAEwWkLUrQu280+De/2UEFq8oB6vjvUJiohremKSNp2Wr8fhL+XQubLoCtw\nln9Pw5EL3607i64Cs5f88Ew35GeKPQw/uUuCI8uB0A==\n=dj6J\n-----END PGP MESSAGE-----\n".to_owned())),
                signature: Some(KeyTokenSignature::from("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwnUEARYKACcFgmYnt8kJkDicqBtFkGUZFiEE5kkQCs8uqswzFfx/OJyoG0WQ\nZRkAACZ4AP49xBDsaIUR1IEJlMqTdwaSJ+02eXXpJANwT/mg2QNTJwD/fXhq\nojjc2LEMrebiFAl4GQgXxkUgnPuvpCyiB80C3A8=\n=KsBO\n-----END PGP SIGNATURE-----\n".to_owned())),
                activation: None,
                primary: true,
                active: true,
                flags: Some(KeyFlag::from(3_u32)),
                recovery_secret: None,
                recovery_secret_signature: None,
                address_forwarding_id: None,
            }]
        ),
        catch_all: false,
        proton_mx: true,
        signed_key_list: ApiAddressSignedKeyList{
            min_epoch_id: Some(3),
            max_epoch_id: Some(66),
            expected_min_epoch_id: None,
            data: Some("[{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"11b13868a3f8e95eb9b76c5fc3e529c7733986eb\",\"SHA256Fingerprints\":[\"f16446135c9380b623bb201a1409bcfd6cb5144fe463b45d08b51e9e335e39ad\",\"ffb76afa704c9a6808bf67009f3a4f0155becf34ff395e3be2e557960b9a4e1c\"]}]".to_owned()),
            obsolescence_token: None,
            signature: Some("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwqkEARYKAFsFgmYnt8kJkMPlKcdzOYbrMxSAAAAAABEAGWNvbnRleHRAcHJv\ndG9uLmNoa2V5LXRyYW5zcGFyZW5jeS5rZXktbGlzdBYhBBGxOGij+Oleubds\nX8PlKcdzOYbrAABnFwD+JukILCsHB7JxsMY4zP9EU8SGhu5/Gwx2aLod9GR1\nfucBANdiI900lTkhTRMHDof4aZ/8Ef5uV1pmQ/CFHQYTcj4P\n=QEZt\n-----END PGP SIGNATURE-----\n".to_owned()),
            revision: 1,
        },
    }]
}

fn test_mail_settings() -> ApiMailSettings {
    let mut settings: ApiMailSettings = ApiMailSettings::default();
    settings.view_mode = ApiViewMode::Messages;
    settings
}
