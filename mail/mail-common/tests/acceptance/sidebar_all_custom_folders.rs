use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType;
use proton_mail_common::Sidebar;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;
use test_case::test_case;
use velcro::hash_map;

#[test_case(&[], &[]; "empty")]
#[test_case(&[
    ("foo",  None,        "foo",  1),
    ("bar",  Some("foo"), "bar",  2),
    ("baz",  Some("foo"), "baz",  3),
    ("titi", None,        "titi", 5),
    ("toto", Some("baz"), "toto", 4),
], &["foo", "bar", "baz", "toto", "titi"]; "hierarchy")]
#[tokio::test]
async fn sidebar_all_custom_folders(labels: &[(&str, Option<&str>, &str, u32)], expected: &[&str]) {
    // Setup:
    //   * Setup User:
    //     + Create Custom Folders
    //   * Create Sidebar
    let ctx = MailTestContext::new().await;
    ctx.setup_user(sidebar_test_params(labels)).await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    let stash = user_ctx.user_stash();
    let tether = stash.connection().await.unwrap();

    // Action
    let result = Sidebar.all_custom_folders(&tether).await.unwrap();

    // Tests
    let result: Vec<_> = result.into_iter().map(|l| l.name).collect();
    assert_eq!(result, expected);
}

fn sidebar_test_params(labels: &[(&str, Option<&str>, &str, u32)]) -> TestParams {
    TestParams {
        labels: hash_map! { LabelType::Folder: labels.iter().map(create_label).collect()},
        ..Default::default()
    }
}

fn create_label((id, parent_id, name, order): &(&str, Option<&str>, &str, u32)) -> ApiLabel {
    ApiLabel {
        id: LabelId::from(*id),
        parent_id: parent_id.map(LabelId::from),
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: LabelType::Folder,
        name: name.to_owned().to_owned(),
        notify: false,
        order: order.to_owned(),
        path: None,
        sticky: false,
    }
}
