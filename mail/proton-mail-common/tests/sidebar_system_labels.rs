mod common;

use crate::common::account::TEST_USER_ID;
use crate::common::init::{NullCallback, Params as TestParams};
use crate::common::test_user_secret;
use common::TestContext;
use proton_api_mail::services::proton::common::{LabelType as ApiLabelType, LabelType};
use proton_api_mail::services::proton::response_data::{
    AlmostAllMail, Label as ApiLabel, MailSettings as ApiMailSettings,
    MessageCount as ApiMessageCount, ShowMoved,
};
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::Sidebar;
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
fn sidebar_system_labels(
    almost_all_mail: AlmostAllMail,
    show_moved: ShowMoved,
    scheduled: bool,
    outbox: bool,
    snoozed: bool,
    expected: &[LabelId],
) {
    tokio_test::block_on(async {
        // Setup:
        //   * Setup User:
        //     + Create MailSettings
        //     + Create all system mailbox
        //     + Add message where needed
        //   * Create Sidebar
        let ctx = TestContext::with_user_secret_and_user_id(
            test_user_secret(),
            RemoteId::from(TEST_USER_ID),
        )
        .await;
        ctx.setup_user(sidebar_test_params(
            almost_all_mail,
            show_moved,
            scheduled,
            outbox,
            snoozed,
        ))
        .await;

        ctx.catch_all().await;

        let user_ctx = ctx.user_context().await;
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &NullCallback {})
            .await
            .unwrap();
        let sidebar = Sidebar::new(user_ctx);

        // Action
        let result = sidebar.system_labels().await.unwrap();

        // Tests
        let result: Vec<_> = result
            .iter()
            .map(|l| l.remote_id.clone().unwrap())
            .collect();
        assert_eq!(result, expected);
    })
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
            label_id: LabelId::all_scheduled().into_inner().into(),
            total: 1,
            unread: 0,
        });
    }

    if outbox {
        message_count.push(ApiMessageCount {
            label_id: LabelId::outbox().into_inner().into(),
            total: 1,
            unread: 0,
        });
    }

    if snoozed {
        message_count.push(ApiMessageCount {
            label_id: LabelId::snoozed().into_inner().into(),
            total: 1,
            unread: 0,
        });
    }

    TestParams {
        mail_settings: Some(sidebar_test_mail_settings(almost_all_mail, show_moved)),
        labels: hash_map! { ApiLabelType::System: vec![
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
        id: label_id.into_inner().into(),
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
