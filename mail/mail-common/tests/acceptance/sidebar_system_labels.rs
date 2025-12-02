use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType;
use proton_core_common::models::{Label, ModelIdExtension};
use proton_mail_api::services::proton::response_data::{
    AlmostAllMail, MailSettings as ApiMailSettings, MessageCount as ApiMessageCount, ShowMoved,
};
use proton_mail_common::Sidebar;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;
use std::default::Default;
use test_case::test_case;
use velcro::hash_map;

#[test_case(AlmostAllMail::AlmostAllMail, ShowMoved::DoNotKeep, true, true, true, &[
    LabelId::inbox(),
    LabelId::drafts(),
    LabelId::all_scheduled(),
    LabelId::outbox(),
    LabelId::snoozed(),
    LabelId::starred(),
    LabelId::sent(),
    LabelId::spam(),
    LabelId::archive(),
    LabelId::trash(),
    LabelId::almost_all_mail(),
] ; "almost_unkeep_scheduled_outbox_snoozed")]
#[test_case(AlmostAllMail::AllMail, ShowMoved::KeepBoth, false, false, false, &[
    LabelId::inbox(),
    LabelId::all_drafts(),
    LabelId::starred(),
    LabelId::all_sent(),
    LabelId::spam(),
    LabelId::archive(),
    LabelId::trash(),
    LabelId::all_mail(),
] ; "all_keep_unscheduled_no_outbox_not_snoozed")]
#[tokio::test]
async fn sidebar_system_labels(
    almost_all_mail: AlmostAllMail,
    show_moved: ShowMoved,
    scheduled: bool,
    outbox: bool,
    snoozed: bool,
    expected: &[LabelId],
) {
    // Setup:
    //   * Setup User:
    //     + Create MailSettings
    //     + Create all system mailbox
    //     + Add message where needed
    //   * Create Sidebar
    let ctx = MailTestContext::new().await;
    ctx.setup_user(sidebar_test_params(
        almost_all_mail,
        show_moved,
        scheduled,
        outbox,
        snoozed,
    ))
    .await;

    let user_ctx = ctx.mail_user_context().await;

    let stash = user_ctx.user_stash();
    let tether = stash.connection().await.unwrap();

    // Action
    let result = Sidebar.system_labels(&tether).await.unwrap();

    // Tests
    let result: Vec<_> = result.iter().map(|l| l.local_id).collect();
    let mut to_expect = Vec::with_capacity(expected.len());
    let tether = user_ctx.user_stash().connection().await.unwrap();
    for label_id in expected {
        to_expect.push(
            Label::remote_id_counterpart(label_id.clone(), &tether)
                .await
                .unwrap()
                .unwrap(),
        )
    }
    assert_eq!(result, to_expect);
}

fn sidebar_test_params(
    almost_all_mail: AlmostAllMail,
    show_moved: ShowMoved,
    scheduled: bool,
    outbox: bool,
    snoozed: bool,
) -> TestParams {
    let mut message_count = vec![];

    if scheduled {
        message_count.push(ApiMessageCount {
            label_id: LabelId::all_scheduled(),
            total: 1,
            unread: 0,
        });
    }

    if outbox {
        message_count.push(ApiMessageCount {
            label_id: LabelId::outbox(),
            total: 1,
            unread: 0,
        });
    }

    if snoozed {
        message_count.push(ApiMessageCount {
            label_id: LabelId::snoozed(),
            total: 1,
            unread: 0,
        });
    }

    TestParams {
        mail_settings: Some(sidebar_test_mail_settings(almost_all_mail, show_moved)),
        labels: hash_map! { LabelType::System: vec![
            create_label(LabelId::all_drafts()),
            create_label(LabelId::all_mail()),
            create_label(LabelId::all_scheduled()),
            create_label(LabelId::all_sent()),
            create_label(LabelId::almost_all_mail()),
            create_label(LabelId::archive()),
            create_label(LabelId::drafts()),
            create_label(LabelId::inbox()),
            create_label(LabelId::outbox()),
            create_label(LabelId::sent()),
            create_label(LabelId::snoozed()),
            create_label(LabelId::spam()),
            create_label(LabelId::starred()),
            create_label(LabelId::trash()),
        ]},
        message_count,
        ..Default::default()
    }
}

fn sidebar_test_mail_settings(
    almost_all_mail: AlmostAllMail,
    show_moved: ShowMoved,
) -> ApiMailSettings {
    ApiMailSettings {
        almost_all_mail,
        show_moved,
        ..Default::default()
    }
}

fn create_label(label_id: LabelId) -> ApiLabel {
    ApiLabel {
        id: label_id,
        label_type: LabelType::System,
        parent_id: None,
        color: "".to_string(),
        display: false,
        expanded: false,
        name: "".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    }
}
