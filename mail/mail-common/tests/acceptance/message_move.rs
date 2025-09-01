use itertools::Itertools as _;
use proton_core_api::services::proton::{
    Address as ApiAddress, DelinquentState, Flags as ApiFlags, Label as ApiLabel,
    ProductUsedSpace as ApiProductUsedSpace, User as ApiUser,
    UserMnemonicStatus as ApiUserMnemonicStatus, UserType as ApiUserType,
};
use proton_core_api::services::proton::{AddressId, LabelId, LabelType as ApiLabelType, UserId};
use proton_core_common::models::{Label, ModelExtension as _, ModelIdExtension as _};
use proton_core_common::test_utils::addresses::ApiAddressTestUtils;
use proton_crypto_account::keys::{ArmoredPrivateKey, KeyId, LockedKey, UserKeys as ApiUserKeys};
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageBody as ApiMessageBody,
    MessageCount as ApiMessageCount, MessageFlags as ApiMessageFlags,
    MessageMetadata as ApiMessageMetadata, MimeType as ApiMimeType, ViewMode as ApiViewMode,
};
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, ConversationCounters, Message, MessageCounters};
use proton_mail_common::test_utils::conversations::ApiConversationTestUtils;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::scroller::StoreLabeledModelMap as _;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use proton_mail_common::{Mailbox, conv_id, conversation, message, msg_id};
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
    let params = test_init_params_labels(labels);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        source_label_id.clone(),
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

    let source = Label::find_first("WHERE remote_id = ?", params!["source"], &tether)
        .await
        .unwrap()
        .unwrap();

    let mut source_conv = ConversationCounters::new(source.id());
    source_conv.total = 1;

    let mut source_msg = MessageCounters::new(source.id());
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

    let mut message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message.label_ids, vec![source_label_id.clone()]);

    // Action:
    // * move message in the other folder
    Message::action_move(
        &tether,
        user_ctx.action_queue(),
        destination.id(),
        vec![message.id()],
    )
    .await
    .unwrap()
    .unwrap();

    message.reload(&tether).await.unwrap();
    assert_eq!(message.label_ids, vec![destination_label_id.clone()]);
}

#[tokio::test]
async fn move_between_folders_and_undo() {
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
    let params = test_init_params_labels(labels);
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
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        source_label_id.clone(),
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

    let source = Label::find_first("WHERE remote_id = ?", params!["source"], &tether)
        .await
        .unwrap()
        .unwrap();

    let mut source_conv = ConversationCounters::new(source.id());
    source_conv.total = 1;

    let mut source_msg = MessageCounters::new(source.id());
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

    let mut message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message.label_ids, vec![source_label_id.clone()]);

    // Action:
    // * move message in the other folder
    let undo = Message::action_move(
        &tether,
        user_ctx.action_queue(),
        destination.id(),
        vec![message.id()],
    )
    .await
    .unwrap()
    .unwrap();

    message.reload(&tether).await.unwrap();
    assert_eq!(message.label_ids, vec![destination_label_id.clone()]);

    undo.undo(user_ctx.action_queue(), &mut tether)
        .await
        .unwrap();
    message.reload(&tether).await.unwrap();
    assert_eq!(message.label_ids, vec![source_label_id.clone()]);

    let undo = Message::action_move(
        &tether,
        user_ctx.action_queue(),
        destination.id(),
        vec![message.id()],
    )
    .await
    .unwrap()
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    // Validation:
    // * the message is in the second folder
    // * the message is not in the first folder
    message.reload(&tether).await.unwrap();
    assert_eq!(message.label_ids, vec![destination_label_id]);

    undo.undo(user_ctx.action_queue(), &mut tether)
        .await
        .unwrap();
    message.reload(&tether).await.unwrap();
    assert_eq!(message.label_ids, vec![source_label_id]);
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
    let destination_label = test_label(&destination_label_id, ApiLabelType::Folder, "destination");
    let message = test_message(vec![LabelId::inbox(), source_label_id.clone()], true);
    let labels = hash_map! {
        ApiLabelType::Label: vec![ source_label, destination_label ]
    };
    let params = test_init_params_labels(labels);
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

    let destination = Label::find_first("WHERE remote_id = ?", params!["destination"], &tether)
        .await
        .unwrap()
        .unwrap();

    let mut message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(message.label_ids.contains(&source_label_id));
    assert_eq!(message.custom_labels.len(), 1);
    assert_eq!(message.custom_labels[0].name, "source");

    // Action:
    // * move message in the other label
    Message::action_move(
        &tether,
        user_ctx.action_queue(),
        destination.id(),
        vec![message.id()],
    )
    .await
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    // Validation:
    // * the message is in the second label
    // * the message is still in the first label
    message.reload(&tether).await.unwrap();
    assert_eq!(
        message.label_ids,
        vec![source_label_id, destination_label_id]
    );
}

#[tokio::test]
async fn move_into_trash_remove_label_and_mark_read() {
    // Setup:
    // * create a message in inbox (or any non-trash mailbox)
    // * add the label to the message
    // * the message is unread
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let trash = Label::find_first("WHERE remote_id = ?", params![LabelId::trash()], &tether)
        .await
        .unwrap()
        .unwrap();

    let label_set1 = vec![
        LabelId::inbox(),
        LabelId::starred(),
        LabelId::all_mail(),
        LabelId::almost_all_mail(),
    ];
    let label_set2 = vec![LabelId::trash(), LabelId::all_mail()];

    let message = test_message(label_set1.clone(), true);
    let params = test_init_params();
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.mock_label_messages(&LabelId::trash(), vec![message.metadata.id.clone()])
        .await;
    ctx.mock_label_messages(&LabelId::starred(), vec![message.metadata.id.clone()])
        .await;
    ctx.catch_all().await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

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

    let mut message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message.label_ids, label_set1);
    assert!(message.unread);

    // Action:
    // * move message in trash and undo it
    let undo = Message::action_move(
        &tether,
        user_ctx.action_queue(),
        trash.local_id.unwrap(),
        vec![message.local_id.unwrap()],
    )
    .await
    .unwrap()
    .unwrap();

    message.reload(&tether).await.unwrap();
    assert_eq!(message.label_ids, label_set2);
    assert!(!message.unread);

    undo.undo(user_ctx.action_queue(), &mut tether)
        .await
        .unwrap();

    message.reload(&tether).await.unwrap();
    assert_eq!(message.label_ids, label_set1);
    assert!(message.unread);

    // Action:
    // * move message in trash and undo it (remotely)
    let undo = Message::action_move(
        &tether,
        user_ctx.action_queue(),
        trash.local_id.unwrap(),
        vec![message.local_id.unwrap()],
    )
    .await
    .unwrap()
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    message.reload(&tether).await.unwrap();
    assert_eq!(message.label_ids, label_set2);
    assert!(!message.unread);

    undo.undo(user_ctx.action_queue(), &mut tether)
        .await
        .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    message.reload(&tether).await.unwrap();
    assert_eq!(message.label_ids, label_set1);
    assert!(message.unread);
}

#[tokio::test]
async fn move_into_spam_remove_labels() {
    // Setup:
    // * create a label
    // * create a message in inbox (or any non-spam mailbox)
    // * add the label to the message
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

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
    let params = test_init_params_labels(labels);
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.mock_label_messages(&spam.remote_id.unwrap(), vec![message.metadata.id.clone()])
        .await;
    ctx.catch_all().await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

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

    let mut message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert!(message.label_ids.contains(&custom_label_id));

    // Action:
    // * move message in spam
    Message::action_move(
        &tether,
        user_ctx.action_queue(),
        spam.local_id.unwrap(),
        vec![message.local_id.unwrap()],
    )
    .await
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    // Validation:
    // * the message only has the `all_mail` label (and spam)
    message.reload(&tether).await.unwrap();
    assert_eq!(
        message.label_ids,
        vec![LabelId::spam(), LabelId::all_mail()]
    );
}

#[tokio::test]
async fn move_out_of_spam_set_almost_all_mail() {
    // Setup:
    // * create a message in spam
    // * the message doesn't have `almost_all_mail` label
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let spam = Label::find_first("WHERE remote_id = ?", params![LabelId::spam()], &tether)
        .await
        .unwrap()
        .unwrap();

    let mut spam_conv = ConversationCounters::new(spam.id());
    spam_conv.total = 1;

    let mut spam_msg = MessageCounters::new(spam.id());
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
    let params = test_init_params_labels(HashMap::new());
    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.mock_label_messages(&inbox.remote_id.unwrap(), vec![message.metadata.id.clone()])
        .await;
    ctx.catch_all().await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::spam(),
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

    let mut message = Message::load(1.into(), &tether).await.unwrap().unwrap();
    assert_eq!(message.label_ids.len(), 1);
    assert_eq!(message.label_ids[0].as_str(), "4");

    // Action:
    // * move message out of spam
    Message::action_move(
        &tether,
        user_ctx.action_queue(),
        inbox.local_id.unwrap(),
        vec![message.local_id.unwrap()],
    )
    .await
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    // Validation:
    // * the message have `almost_all_mail` label
    message.reload(&tether).await.unwrap();
    assert_eq!(
        message.label_ids,
        vec![LabelId::inbox(), LabelId::almost_all_mail()]
    );
}

#[tokio::test]
async fn move_message_also_moves_conversation() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    ctx.setup_user(params.clone()).await;
    ctx.mock_label_messages(&LabelId::spam(), vec!["my_message".into()])
        .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    let tether = &mut user_ctx.user_stash().connection().await.unwrap();

    let mut conv_data = hash_map! {
        vec![LabelId::inbox()]: vec![conversation!(remote_id: conv_id!("my_conv"))]
    };
    conv_data.save_to_database(tether).await;

    let conv = &conv_data.get(&vec![LabelId::inbox()]).unwrap()[0];

    let mut msg_data = (
        LabelId::inbox(),
        vec![message!(
                remote_id: msg_id!("my_message"),
                local_conversation_id: conv.local_id,
                remote_conversation_id: conv.remote_id.clone())],
    );
    msg_data.save_to_database(tether).await;

    // ---
    let local_inbox = Label::remote_id_counterpart(LabelId::inbox(), tether)
        .await
        .unwrap()
        .unwrap();

    let local_spam = Label::remote_id_counterpart(LabelId::spam(), tether)
        .await
        .unwrap()
        .unwrap();

    let convs = Conversation::in_label(local_inbox, tether).await.unwrap();
    assert_eq!(convs.len(), 1);
    conv_is_labeled(&convs[0], [LabelId::inbox()]);

    let msgs = Message::in_label(local_inbox, tether).await.unwrap();
    assert_eq!(msgs.len(), 1);
    let message = &msgs[0];
    assert_eq!(message.label_ids, vec![LabelId::inbox()]);

    assert_eq!(
        message.exclusive_location.as_ref().unwrap().local_id(),
        local_inbox
    );

    // Action:
    // * move message in the other folder
    let undo = Message::action_move(
        tether,
        user_ctx.action_queue(),
        local_spam,
        vec![message.id()],
    )
    .await
    .unwrap()
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    // recheck but in the destination folder
    let msgs = Message::in_label(local_spam, tether).await.unwrap();
    let convs = Conversation::in_label(local_spam, tether).await.unwrap();

    let message = &msgs[0];
    assert_eq!(
        message.label_ids,
        vec![LabelId::spam(), LabelId::all_mail()]
    );

    assert_eq!(
        message.exclusive_location.as_ref().unwrap().local_id(),
        local_spam
    );

    conv_is_labeled(&convs[0], [LabelId::all_mail(), LabelId::spam()]);

    undo.undo(user_ctx.action_queue(), tether).await.unwrap();
    let convs = Conversation::in_label(local_inbox, tether).await.unwrap();
    conv_is_labeled(
        &convs[0],
        [
            LabelId::all_mail(),
            LabelId::inbox(),
            LabelId::almost_all_mail(),
        ],
    );

    let msgs = Message::in_label(local_inbox, tether).await.unwrap();
    assert_eq!(msgs.len(), 1);
    let message = &msgs[0];
    assert_eq!(
        message.label_ids,
        vec![
            LabelId::inbox(),
            // These are not in the test setup, they get added automatically by the actions
            LabelId::all_mail(),
            LabelId::almost_all_mail()
        ]
    );

    assert_eq!(
        message.exclusive_location.as_ref().unwrap().local_id(),
        local_inbox
    );
}

#[tokio::test]
async fn move_conversation_between_folders_and_undo() {
    let ctx = MailTestContext::new().await;

    let conversation = ApiConversation::test_conversation_in_inbox("first", vec![]);

    let conversation_count = vec![ApiConversationCount {
        label_id: LabelId::inbox().clone(),
        total: 1,
        unread: 0,
    }];
    let message_count = vec![ApiMessageCount {
        label_id: LabelId::inbox().clone(),
        total: 1,
        unread: 0,
    }];
    let params = TestParams {
        addresses: vec![ApiAddress::test_address()],
        conversations: vec![conversation.clone()],
        conversation_count,
        message_count,
        ..Default::default()
    };
    ctx.setup_user(params).await;

    ctx.mock_label_conversation(
        &LabelId::inbox(),
        vec![conversation.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_label_conversation(
        &LabelId::archive(),
        vec![conversation.id.clone()],
        None,
        vec![],
    )
    .await;

    ctx.mock_get_conversations(vec![conversation], 1).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

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

    let local_archive = Label::resolve_local_label_id(LabelId::archive(), &tether)
        .await
        .unwrap();

    let mut conv = Conversation::load(1.into(), &tether)
        .await
        .unwrap()
        .unwrap();

    conv_is_labeled(&conv, [LabelId::inbox()]);

    // Action:
    // * move message in the other folder
    let undo = Conversation::action_move(
        &tether,
        user_ctx.action_queue(),
        local_archive,
        vec![conv.id()],
    )
    .await
    .unwrap()
    .unwrap();

    conv.reload(&tether).await.unwrap();
    conv_is_labeled(&conv, [LabelId::archive()]);

    undo.undo(user_ctx.action_queue(), &mut tether)
        .await
        .unwrap();
    conv.reload(&tether).await.unwrap();
    conv_is_labeled(&conv, [LabelId::inbox()]);

    let undo = Conversation::action_move(
        &tether,
        user_ctx.action_queue(),
        local_archive,
        vec![conv.id()],
    )
    .await
    .unwrap()
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    conv.reload(&tether).await.unwrap();
    conv_is_labeled(&conv, [LabelId::archive()]);

    undo.undo(user_ctx.action_queue(), &mut tether)
        .await
        .unwrap();
    user_ctx.execute_all_actions().await.unwrap();
    conv.reload(&tether).await.unwrap();
    conv_is_labeled(&conv, [LabelId::inbox()]);
}

#[tokio::test]
async fn move_conversation_mix_unread() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::new().await;
    let conversation_count = vec![ApiConversationCount {
        label_id: LabelId::inbox().clone(),
        total: 1,
        unread: 1,
    }];
    let message_count = vec![ApiMessageCount {
        label_id: LabelId::inbox().clone(),
        total: 3,
        unread: 2,
    }];
    let params = TestParams {
        addresses: vec![ApiAddress::test_address()],
        conversation_count,
        message_count,
        ..Default::default()
    };
    ctx.setup_user(params).await;

    let user_ctx = ctx.mail_user_context().await;

    let tether = &mut user_ctx.user_stash().connection().await.unwrap();

    let mut conv_data = hash_map! {
        vec![LabelId::inbox()]: vec![conversation!(remote_id: conv_id!("my_conv"))]
    };

    conv_data.save_to_database(tether).await;

    let conv = &conv_data.get(&vec![LabelId::inbox()]).unwrap()[0];

    let mut msg_data = (
        LabelId::inbox(),
        vec![
            message!(
                remote_id: msg_id!("1"),
                unread: false,
                local_conversation_id: conv.local_id,
                remote_conversation_id: conv.remote_id.clone()),
            message!(
                remote_id: msg_id!("2"),
                unread: true,
                local_conversation_id: conv.local_id,
                remote_conversation_id: conv.remote_id.clone()),
            message!(
                remote_id: msg_id!("3"),
                unread: true,
                local_conversation_id: conv.local_id,
                remote_conversation_id: conv.remote_id.clone()),
        ],
    );
    msg_data.save_to_database(tether).await;

    ctx.mock_label_conversation(&LabelId::trash(), vec!["my_conv".into()], None, vec![])
        .await;
    ctx.catch_all().await;

    // ---
    let local_inbox = Label::remote_id_counterpart(LabelId::inbox(), tether)
        .await
        .unwrap()
        .unwrap();

    let local_trash = Label::remote_id_counterpart(LabelId::trash(), tether)
        .await
        .unwrap()
        .unwrap();

    {
        let convs = Conversation::in_label(local_inbox, tether).await.unwrap();

        assert_eq!(convs.len(), 1);

        let conv = &convs[0];
        assert_eq!(conv.num_messages, 3);
        assert_eq!(conv.num_unread, 2);

        conv_is_labeled(conv, [LabelId::inbox()]);

        let msgs = Message::in_label(local_inbox, tether).await.unwrap();
        let unreads = msgs.iter().map(|m| m.unread).collect_vec();
        assert_eq!(unreads, vec![false, true, true]);
        for message in msgs {
            assert_eq!(message.label_ids, vec![LabelId::inbox()]);
        }
    }

    // Action:
    let undo = Conversation::action_move(
        tether,
        user_ctx.action_queue(),
        local_trash,
        vec![conv.id()],
    )
    .await
    .unwrap()
    .unwrap();
    user_ctx.execute_all_actions().await.unwrap();

    {
        let convs = Conversation::in_label(local_trash, tether).await.unwrap();

        assert_eq!(convs.len(), 1);

        let conv = &convs[0];
        assert_eq!(conv.num_messages, 3);
        assert_eq!(conv.num_unread, 0);

        conv_is_labeled(conv, [LabelId::all_mail(), LabelId::trash()]);

        let msgs = Message::in_label(local_trash, tether).await.unwrap();
        let unreads = msgs.iter().map(|m| m.unread).collect_vec();
        assert_eq!(unreads, vec![false, false, false]);
        for message in msgs {
            assert_eq!(
                message.label_ids,
                vec![LabelId::trash(), LabelId::all_mail()]
            );
        }
    }

    undo.undo(user_ctx.action_queue(), tether).await.unwrap();

    {
        let convs = Conversation::in_label(local_inbox, tether).await.unwrap();

        assert_eq!(convs.len(), 1);

        let conv = &convs[0];
        assert_eq!(conv.num_messages, 3);
        assert_eq!(conv.num_unread, 2);

        conv_is_labeled(
            conv,
            [
                LabelId::all_mail(),
                LabelId::inbox(),
                LabelId::almost_all_mail(),
            ],
        );

        let msgs = Message::in_label(local_inbox, tether).await.unwrap();
        let unreads = msgs.iter().map(|m| m.unread).collect_vec();
        assert_eq!(unreads, vec![false, true, true]);
        for message in msgs {
            assert_eq!(
                message.label_ids,
                vec![
                    LabelId::inbox(),
                    LabelId::all_mail(),
                    LabelId::almost_all_mail(),
                ]
            );
        }
    }
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
            reply_to: Default::default(),
            mime_type: ApiMimeType::TextHtml,
            attachments: vec![],
            reply_tos: vec![],
        },
    }
}

fn test_init_params_labels(labels: HashMap<ApiLabelType, Vec<ApiLabel>>) -> TestParams {
    TestParams {
        labels,
        ..test_init_params()
    }
}

fn test_init_params() -> TestParams {
    TestParams {
        user_info: Some(test_user_info()),
        addresses: ApiAddress::test_addresses(),
        mail_settings: Some(test_mail_settings()),
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

#[track_caller]
fn conv_is_labeled(conv: &Conversation, expected_labels: impl Into<Vec<LabelId>>) {
    let mut expected_labels: Vec<_> = expected_labels.into();
    let mut actual_labels = conv
        .labels
        .iter()
        .map(|l| l.remote_label_id.clone().unwrap())
        .collect_vec();

    expected_labels.sort();
    actual_labels.sort();

    assert_eq!(actual_labels, expected_labels);
}
