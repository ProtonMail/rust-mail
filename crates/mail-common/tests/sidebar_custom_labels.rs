use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::{LabelType as ApiLabelType, LabelType};
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_mail_common::Sidebar;
use proton_mail_test_utils::common::TestContext;
use proton_mail_test_utils::init::Params as TestParams;
use test_case::test_case;
use velcro::hash_map;

#[test_case(&[], &[]; "empty")]
#[test_case(&[(ApiRemoteId::from("foo"), "foo".to_owned(), 42)], &["foo".to_owned()]; "single")]
#[test_case(&[
    (ApiRemoteId::from("bar"), "bar".to_owned(), 2),
    (ApiRemoteId::from("baz"), "baz".to_owned(), 3),
    (ApiRemoteId::from("foo"), "foo".to_owned(), 1),
    (ApiRemoteId::from("titi"), "titi".to_owned(), 5),
    (ApiRemoteId::from("toto"), "toto".to_owned(), 4),
], &[
    "foo".to_owned(),
    "bar".to_owned(),
    "baz".to_owned(),
    "toto".to_owned(),
    "titi".to_owned()
]; "many")]
#[tokio::test]
async fn sidebar_custom_labels(labels: &[(ApiRemoteId, String, u32)], expected: &[String]) {
    // Setup:
    //   * Setup User:
    //     + Create Custom Folders
    //   * Create Sidebar

    let ctx = TestContext::new().await;
    ctx.setup_user(sidebar_test_params(labels)).await;

    ctx.catch_all().await;

    let user_ctx = ctx.user_context().await;
    ctx.init_user(user_ctx.clone()).await;
    let sidebar = Sidebar::new(user_ctx);

    // Action
    let result = sidebar.custom_labels().await.unwrap();

    // Tests
    let result: Vec<_> = result.into_iter().map(|l| l.name).collect();
    assert_eq!(result, expected);
}

fn sidebar_test_params(labels: &[(ApiRemoteId, String, u32)]) -> TestParams {
    TestParams {
        labels: hash_map! { ApiLabelType::Label: labels.iter().map(create_label).collect()},
        ..Default::default()
    }
}

fn create_label((id, name, order): &(ApiRemoteId, String, u32)) -> ApiLabel {
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
