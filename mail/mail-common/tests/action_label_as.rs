use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    ConversationLabel as ApiConversationLabel, Label as ApiLabel, MessageCount as ApiMessageCount,
};
use proton_core_common::datatypes::{Id, LabelId};
use proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, ArmoredPrivateKey, EncryptedKeyToken, KeyFlag, KeyId,
    KeyTokenSignature, LockedKey,
};
use proton_mail_common::datatypes::{ExclusiveLocation, SystemLabel, SystemLabelId};
use proton_mail_common::models::{Conversation, Label};
use proton_mail_common::Mailbox;
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::test_context::MailTestContext;
use stash::orm::Model;
use stash::params;
use std::collections::{HashMap, HashSet};
use velcro::{hash_map, hash_set};

const TEST_USER_ADDRESS_ID: &str =
    "LGXtB3TbNifsW1elXtCp5zyysma52yRf8NZZ10pUQrJfp1QQCSoFTXcIVDCZJycme6KYHsxCE_xdneJ10dt_iA==";

#[tokio::test]
async fn action_label_as_without_archive() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();

    let inbox_label = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], stash)
        .await
        .unwrap()
        .unwrap();

    let label1_id = LabelId::from("selected");
    let label1 = test_label(&label1_id, "selected");
    let label2_id = LabelId::from("partial");
    let label2 = test_label(&label2_id, "partial");
    let label3_id = LabelId::from("unselected");
    let label3 = test_label(&label3_id, "unselected");
    let labels = hash_map! {
        ApiLabelType::Label: vec![label1.clone(), label2.clone(), label3.clone()],
    };

    let conversation1 = test_conversation("first", vec![]);
    let conversation2 = test_conversation("second", vec![label2.clone(), label3.clone()]);
    let conversation3 = test_conversation("third", vec![label1.clone(), label3.clone()]);
    let conversation4 = test_conversation(
        "fourth",
        vec![label1.clone(), label2.clone(), label3.clone()],
    );
    let conversations = vec![
        conversation1.clone(),
        conversation2.clone(),
        conversation3.clone(),
        conversation4.clone(),
    ];

    let params = test_init_params(labels, conversations.clone());
    ctx.setup_user(params).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_label_conversation(
        &label1_id.clone().into_inner().into(),
        vec![conversation1.id.clone(), conversation2.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_unlabel_conversation(
        &label3_id.into_inner().into(),
        vec![
            conversation2.id,
            conversation3.id.clone(),
            conversation4.id.clone(),
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
    label1.total_conv = 2;
    label1.save_using(stash).await.unwrap();
    let mut label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], stash)
        .await
        .unwrap()
        .unwrap();
    label2.total_conv = 2;
    label2.save_using(stash).await.unwrap();
    let mut label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], stash)
        .await
        .unwrap()
        .unwrap();
    label3.total_conv = 3;
    label3.save_using(stash).await.unwrap();

    let conversation1 = Conversation::load(1.into(), stash).await.unwrap().unwrap();
    assert!(conversation1.labels.is_empty());
    let conversation2 = Conversation::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation2.labels.len(), 2);
    let conversation3 = Conversation::load(3.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation3.labels.len(), 2);
    let conversation4 = Conversation::load(4.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation4.labels.len(), 3);

    // Action
    Conversation::action_label_as(
        user_ctx.queue(),
        inbox_label.local_id.unwrap(),
        vec![
            conversation1.local_id.unwrap(),
            conversation2.local_id.unwrap(),
            conversation3.local_id.unwrap(),
            conversation4.local_id.unwrap(),
        ],
        vec![label1.local_id.unwrap()],
        vec![label2.local_id.unwrap()],
        false,
    )
    .await
    .unwrap();

    // Validation
    let conversation1 = Conversation::load(1.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation1.labels.len(), 1);
    let ids: HashSet<_> = conversation1
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.local_id.unwrap()]);
    let conversation2 = Conversation::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation2.labels.len(), 2);
    let ids: HashSet<_> = conversation2
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(
        ids,
        hash_set![label1.local_id.unwrap(), label2.local_id.unwrap(),]
    );
    let conversation3 = Conversation::load(3.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation3.labels.len(), 1);
    let ids: HashSet<_> = conversation3
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.local_id.unwrap(),]);
    let conversation4 = Conversation::load(4.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation4.labels.len(), 2);
    let ids: HashSet<_> = conversation4
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(
        ids,
        hash_set![label1.local_id.unwrap(), label2.local_id.unwrap(),]
    );

    let label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label1.total_conv, 4);
    let label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label2.total_conv, 2);
    let label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label3.total_conv, 0);
}

#[tokio::test]
async fn action_label_as_with_archive() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();

    let inbox_label = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], stash)
        .await
        .unwrap()
        .unwrap();

    let label1_id = LabelId::from("selected");
    let label1 = test_label(&label1_id, "selected");
    let label2_id = LabelId::from("partial");
    let label2 = test_label(&label2_id, "partial");
    let label3_id = LabelId::from("unselected");
    let label3 = test_label(&label3_id, "unselected");
    let labels = hash_map! {
        ApiLabelType::Label: vec![label1.clone(), label2.clone(), label3.clone()],
    };

    let conversation1 = test_conversation("first", vec![]);
    let conversation2 = test_conversation(
        "second",
        vec![label1.clone(), label2.clone(), label3.clone()],
    );
    let conversations = vec![conversation1.clone(), conversation2.clone()];

    let params = test_init_params(labels, conversations.clone());
    ctx.setup_user(params).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_label_conversation(
        &LabelId::archive().into(),
        vec![conversation1.id.clone(), conversation2.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_label_conversation(
        &label1_id.clone().into_inner().into(),
        vec![conversation1.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_unlabel_conversation(
        &label3_id.into_inner().into(),
        vec![conversation2.id],
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
    label1.total_conv = 1;
    label1.save_using(stash).await.unwrap();
    let mut label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], stash)
        .await
        .unwrap()
        .unwrap();
    label2.total_conv = 1;
    label2.save_using(stash).await.unwrap();
    let mut label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], stash)
        .await
        .unwrap()
        .unwrap();
    label3.total_conv = 1;
    label3.save_using(stash).await.unwrap();

    let conversation1 = Conversation::load(1.into(), stash).await.unwrap().unwrap();
    assert!(conversation1.labels.is_empty());
    let conversation2 = Conversation::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation2.labels.len(), 3);

    // Action
    Conversation::action_label_as(
        user_ctx.queue(),
        inbox_label.local_id.unwrap(),
        vec![
            conversation1.local_id.unwrap(),
            conversation2.local_id.unwrap(),
        ],
        vec![label1.local_id.unwrap()],
        vec![label2.local_id.unwrap()],
        true,
    )
    .await
    .unwrap();

    // Validation
    let archive_id = LabelId::archive()
        .counterpart::<Label, _>(stash)
        .await
        .unwrap()
        .unwrap();

    let conversation1 = Conversation::load(1.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation1.labels.len(), 2);
    let ids: HashSet<_> = conversation1
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.local_id.unwrap(), archive_id]);
    assert_eq!(
        conversation1.exclusive_location,
        Some(ExclusiveLocation::System {
            name: SystemLabel::Archive,
            local_id: archive_id,
        })
    );
    let conversation2 = Conversation::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation2.labels.len(), 3);
    let ids: HashSet<_> = conversation2
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(
        ids,
        hash_set![
            label1.local_id.unwrap(),
            label2.local_id.unwrap(),
            archive_id
        ]
    );
    assert_eq!(
        conversation2.exclusive_location,
        Some(ExclusiveLocation::System {
            name: SystemLabel::Archive,
            local_id: archive_id,
        })
    );
}

fn test_init_params(
    labels: HashMap<ApiLabelType, Vec<ApiLabel>>,
    conversations: Vec<ApiConversation>,
) -> TestParams {
    let conversation_count = vec![ApiConversationCount {
        label_id: LabelId::inbox().clone().into(),
        total: conversations.len() as u64,
        unread: 0,
    }];
    let message_count = vec![ApiMessageCount {
        label_id: LabelId::inbox().clone().into(),
        total: 1,
        unread: 0,
    }];
    TestParams {
        labels,
        addresses: vec![test_address()],
        conversations,
        conversation_count,
        message_count,
        ..Default::default()
    }
}

fn test_label(label_id: &LabelId, name: &str) -> ApiLabel {
    ApiLabel {
        id: label_id.clone().into(),
        label_type: ApiLabelType::Label,
        name: name.to_owned(),
        ..Default::default()
    }
}

fn test_conversation(id: &str, labels: Vec<ApiLabel>) -> ApiConversation {
    let labels = labels
        .into_iter()
        .map(|l| ApiConversationLabel {
            id: l.id,
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_messages: 1,
            context_num_unread: 0,
            context_size: 0,
            context_snooze_time: 0,
            context_time: 0,
        })
        .collect();
    ApiConversation {
        id: id.into(),
        num_messages: 1,
        labels,
        ..Default::default()
    }
}

fn test_address() -> ApiAddress {
    let lock_key = LockedKey{
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
    };
    let signed_key_list = ApiAddressSignedKeyList{
        min_epoch_id: Some(3),
        max_epoch_id: Some(66),
        expected_min_epoch_id: None,
        data: Some("[{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"11b13868a3f8e95eb9b76c5fc3e529c7733986eb\",\"SHA256Fingerprints\":[\"f16446135c9380b623bb201a1409bcfd6cb5144fe463b45d08b51e9e335e39ad\",\"ffb76afa704c9a6808bf67009f3a4f0155becf34ff395e3be2e557960b9a4e1c\"]}]".to_owned()),
        obsolescence_token: None,
        signature: Some("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwqkEARYKAFsFgmYnt8kJkMPlKcdzOYbrMxSAAAAAABEAGWNvbnRleHRAcHJv\ndG9uLmNoa2V5LXRyYW5zcGFyZW5jeS5rZXktbGlzdBYhBBGxOGij+Oleubds\nX8PlKcdzOYbrAABnFwD+JukILCsHB7JxsMY4zP9EU8SGhu5/Gwx2aLod9GR1\nfucBANdiI900lTkhTRMHDof4aZ/8Ef5uV1pmQ/CFHQYTcj4P\n=QEZt\n-----END PGP SIGNATURE-----\n".to_owned()),
        revision: 1,
    };
    ApiAddress {
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
        keys: ApiAddressKeys(vec![lock_key]),
        catch_all: false,
        proton_mx: true,
        signed_key_list,
    }
}
