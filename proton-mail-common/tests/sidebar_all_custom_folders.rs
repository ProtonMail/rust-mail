use crate::common::init::{NullCallback, Params as TestParams};
use crate::common::TestContext;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::{LabelType as ApiLabelType, LabelType};
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_core_common::datatypes::LabelId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::Sidebar;
use test_case::test_case;
use velcro::hash_map;

mod common;

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
    let ctx = TestContext::new().await;
    ctx.setup_user(sidebar_test_params(labels)).await;

    ctx.catch_all().await;

    let user_ctx = ctx.user_context().await;
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .unwrap();
    let sidebar = Sidebar::new(user_ctx.clone());

    // Action
    let result = sidebar.all_custom_folders().await.unwrap();

    // Tests
    let result: Vec<_> = result.into_iter().map(|l| l.name).collect();
    assert_eq!(result, expected);
}

fn sidebar_test_params(labels: &[(&str, Option<&str>, &str, u32)]) -> TestParams {
    TestParams {
        labels: hash_map! { ApiLabelType::Folder: labels.iter().map(create_label).collect()},
        ..Default::default()
    }
}

fn create_label((id, parent_id, name, order): &(&str, Option<&str>, &str, u32)) -> ApiLabel {
    ApiLabel {
        id: ApiRemoteId::from(*id),
        parent_id: parent_id.map(|p| ApiRemoteId::from(p)),
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
