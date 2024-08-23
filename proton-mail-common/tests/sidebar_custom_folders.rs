use crate::common::init::{NullCallback, Params as TestParams};
use crate::common::TestContext;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::{LabelType as ApiLabelType, LabelType};
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_core_common::datatypes::LabelId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Label;
use proton_mail_common::Sidebar;
use stash::orm::Model;
use stash::params;
use stash::stash::Stash;
use test_case::test_case;
use velcro::hash_map;

mod common;

#[test_case(&[], None, &[]; "empty")]
#[test_case(&[
    ("foo",  None,        "foo", 1),
    ("bar",  Some("foo"), "bar", 2),
    ("titi", None,        "titi",5)
], None, &["foo", "titi"]; "root")]
#[test_case(&[
    ("foo",  None,        "foo",  1),
    ("bar",  Some("foo"), "bar",  2),
    ("baz",  Some("foo"), "baz",  3),
    ("titi", None,        "titi", 5),
    ("toto", Some("baz"), "toto", 4),
], Some("foo"), &["bar", "baz"]; "hierarchy")]
#[tokio::test]
async fn sidebar_custom_folders(
    labels: &[(&str, Option<&str>, &str, u32)],
    parent_id: Option<&str>,
    expected: &[&str],
) {
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

    let parent = get_label(parent_id, user_ctx.user_stash()).await;

    // Action
    let result = sidebar
        .custom_folders(parent.map(|p| p.local_id.unwrap()))
        .await
        .unwrap();

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
        parent_id: parent_id.map(ApiRemoteId::from),
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

async fn get_label(label_name: Option<&str>, stash: &Stash) -> Option<Label> {
    if let Some(name) = label_name {
        Label::find_first("WHERE remote_id = ?", params![LabelId::from(name)], stash)
            .await
            .unwrap()
    } else {
        None
    }
}
