use crate::common::init::{NullCallback, Params as TestParams};
use crate::common::TestContext;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::{LabelType as ApiLabelType, LabelType};
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_core_common::datatypes::LabelId;
use proton_mail_common::datatypes::custom_folder::CustomFolder;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::Sidebar;
use std::iter::zip;
use test_case::test_case;
use velcro::hash_map;

mod common;

#[derive(Clone)]
struct H {
    name: String,
    children: Vec<H>,
}

impl H {
    fn is_matching(&self, folder: CustomFolder) {
        assert_eq!(self.name, folder.name);
        assert_eq!(self.children.len(), folder.children.len());
        zip(self.children.clone(), folder.children).for_each(|(h, f)| h.is_matching(f));
    }
}

#[test_case(&[], &[]; "empty")]
#[test_case(&[
    ("foo",  None, "foo", 1),
    ("bar",  None, "bar", 2),
    ("titi", None, "titi",5)
], &[H{name: "foo".to_owned(), children: vec![]},
     H{name: "bar".to_owned(), children: vec![]},
     H{name: "titi".to_owned(), children: vec![]}]; "root")]
#[test_case(&[
    ("foo",  None,         "foo",  1),
    ("bar",  Some("foo"),  "bar",  2),
    ("baz",  Some("foo"),  "baz",  3),
    ("toto", Some("baz"),  "toto", 4),
    ("titi", None,         "titi", 5),
    ("tutu", Some("titi"), "tutu", 6),
    ("tata", Some("tutu"), "tata", 7),
    ("tete", Some("tutu"), "tete", 8),
    ("tyty", Some("titi"), "tyty", 9),
], &[H{name: "foo".to_owned(), children: vec![
        H{name: "bar".to_owned(), children: vec![]},
        H{name: "baz".to_owned(), children: vec![
            H{name: "toto".to_owned(), children: vec![]}]},
     ]},
     H{name: "titi".to_owned(), children: vec![
        H{name: "tutu".to_owned(), children: vec![
            H{name: "tata".to_owned(), children: vec![]},
            H{name: "tete".to_owned(), children: vec![]},
        ]},
        H{name: "tyty".to_owned(), children: vec![]},
     ]}]; "hierarchy")]
#[tokio::test]
async fn sidebar_custom_folders(labels: &[(&str, Option<&str>, &str, u32)], expected: &[H]) {
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
    let result = sidebar.custom_folders().await.unwrap();

    // Tests
    for (res, h) in zip(result, expected) {
        h.is_matching(res);
    }
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
