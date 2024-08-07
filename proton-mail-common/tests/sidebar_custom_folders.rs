use crate::common::init::{NullCallback, Params as TestParams};
use crate::common::TestContext;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::{LabelType as ApiLabelType, LabelType};
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::Sidebar;
use test_case::test_case;
use velcro::hash_map;

mod common;

#[test_case(&[], &[]; "empty")]
#[test_case(&[("foo", None, "foo", 42)], &[("foo", None, "foo")]; "single")]
#[test_case(&[
    ("bar",  Some("foo"), "bar",  2),
    ("baz",  Some("foo"), "baz",  3),
    ("foo",  None,        "foo",  1),
    ("titi", None,        "titi", 5),
    ("toto", Some("baz"), "toto", 4),
], &[
    ("foo",  None,        "foo"),
    ("bar",  Some("foo"), "bar"),
    ("baz",  Some("foo"), "baz"),
    ("toto", Some("baz"), "toto"),
    ("titi", None,        "titi")
]; "hierarchy")]
fn sidebar_custom_folders(
    labels: &[(&str, Option<&str>, &str, u32)],
    expected: &[(&str, Option<&str>, &str)],
) {
    tokio_test::block_on(async {
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
        let sidebar = Sidebar::new(user_ctx);

        // Action
        let result = sidebar.custom_folders(None).await.unwrap();

        // Tests
        let result: Vec<_> = result
            .into_iter()
            .map(|l| (l.remote_id.unwrap(), l.remote_parent_id, l.name))
            .collect();
        assert_eq!(
            result,
            expected.iter().map(format_expected).collect::<Vec<_>>()
        );
    })
}

fn format_expected(
    (id, parent, name): &(&str, Option<&str>, &str),
) -> (LabelId, Option<LabelId>, String) {
    (
        RemoteId::from(*id).into(),
        parent.map(|p| RemoteId::from(p).into()),
        name.to_owned().to_owned(),
    )
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
