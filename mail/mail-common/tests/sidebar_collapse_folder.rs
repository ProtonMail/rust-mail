use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::{LabelType as ApiLabelType, LabelType};
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_core_common::datatypes::RemoteId;
use proton_mail_common::models::Label;
use proton_mail_common::Sidebar;
use proton_mail_test_utils::common::TestContext;
use proton_mail_test_utils::init::Params;
use proton_mail_test_utils::init::Params as TestParams;
use stash::orm::Model;
use stash::params;
use stash::stash::Stash;
use velcro::hash_map;

#[tokio::test]
async fn folder_expansion() {
    // Setup:
    //   * Setup User:
    //     + Create a Custom Folders not expanded
    //   * Create Sidebar
    let name = "foo";
    let ctx = TestContext::new().await;
    ctx.setup_user(sidebar_test_params(name, false)).await;

    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();
    ctx.init_user(user_ctx.clone()).await;
    let sidebar = Sidebar::new(user_ctx.clone());

    let folder = get_folder("foo", stash).await;
    assert!(!folder.expanded);

    ctx.mock_patch_label(folder.remote_id.unwrap(), true).await;
    ctx.catch_all().await;

    // Action
    sidebar
        .expand_folder(folder.local_id.unwrap())
        .await
        .unwrap();

    // Tests
    let folder = get_folder(name, sidebar.user_ctx.user_stash()).await;
    assert!(folder.expanded);
}

#[tokio::test]
async fn folder_collapse() {
    // Setup:
    //   * Setup User:
    //     + Create a Custom Folders expanded
    //   * Create Sidebar
    let name = "foo";
    let ctx = TestContext::new().await;
    ctx.setup_user(sidebar_test_params(name, true)).await;

    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();
    ctx.init_user(user_ctx.clone()).await;
    let sidebar = Sidebar::new(user_ctx.clone());

    let folder = get_folder("foo", stash).await;
    assert!(folder.expanded);

    ctx.mock_patch_label(folder.remote_id.unwrap(), false).await;
    ctx.catch_all().await;

    // Action
    sidebar
        .collapse_folder(folder.local_id.unwrap())
        .await
        .unwrap();

    // Tests
    let folder = get_folder(name, sidebar.user_ctx.user_stash()).await;
    assert!(!folder.expanded);
}

async fn get_folder(name: &str, stash: &Stash) -> Label {
    Label::find_first("WHERE remote_id = ?", params![RemoteId::from(name)], stash)
        .await
        .unwrap()
        .unwrap()
}

fn sidebar_test_params(name: &str, state: bool) -> Params {
    TestParams {
        labels: hash_map! { ApiLabelType::Folder: vec![ create_label(name, state) ]},
        ..Default::default()
    }
}

fn create_label(name: &str, expanded: bool) -> ApiLabel {
    ApiLabel {
        id: ApiRemoteId::from(name),
        parent_id: None,
        color: "".to_string(),
        display: false,
        expanded,
        label_type: LabelType::Folder,
        name: "".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    }
}
