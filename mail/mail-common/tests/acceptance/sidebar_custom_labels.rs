use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType;
use proton_mail_common::Sidebar;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;
use test_case::test_case;
use velcro::hash_map;

#[test_case(&[], &[]; "empty")]
#[test_case(&[(LabelId::from("foo"), "foo".to_owned(), 42)], &["foo".to_owned()]; "single")]
#[test_case(&[
    (LabelId::from("bar"), "bar".to_owned(), 2),
    (LabelId::from("baz"), "baz".to_owned(), 3),
    (LabelId::from("foo"), "foo".to_owned(), 1),
    (LabelId::from("titi"), "titi".to_owned(), 5),
    (LabelId::from("toto"), "toto".to_owned(), 4),
], &[
    "foo".to_owned(),
    "bar".to_owned(),
    "baz".to_owned(),
    "toto".to_owned(),
    "titi".to_owned()
]; "many")]
#[tokio::test]
async fn sidebar_custom_labels(labels: &[(LabelId, String, u32)], expected: &[String]) {
    // Setup:
    //   * Setup User:
    //     + Create Custom Folders
    //   * Create Sidebar

    let ctx = MailTestContext::new().await;
    ctx.setup_user(sidebar_test_params(labels)).await;

    let user_ctx = ctx.mail_user_context().await;

    let stash = user_ctx.user_stash();
    let tether = stash.connection().await.unwrap();

    // Action
    let result = Sidebar.custom_labels(&tether).await.unwrap();

    // Tests
    let result: Vec<_> = result.into_iter().map(|l| l.name).collect();
    assert_eq!(result, expected);
}

fn sidebar_test_params(labels: &[(LabelId, String, u32)]) -> TestParams {
    TestParams {
        labels: hash_map! { LabelType::Label: labels.iter().map(create_label).collect()},
        ..Default::default()
    }
}

fn create_label((id, name, order): &(LabelId, String, u32)) -> ApiLabel {
    ApiLabel {
        id: id.clone(),
        parent_id: None,
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: LabelType::Label,
        name: name.clone(),
        notify: false,
        order: order.to_owned(),
        path: None,
        sticky: false,
    }
}
