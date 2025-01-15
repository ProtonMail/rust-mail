use proton_api_core::services::proton::common::LabelId;
use proton_api_core::services::proton::common::LabelType;
use proton_api_core::services::proton::response_data::Label as ApiLabel;
use proton_mail_common::models::Label;
use proton_mail_common::Sidebar;
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::init::Params;
use proton_mail_test_utils::test_context::MailTestContext;
use stash::orm::Model;
use stash::params;
use stash::stash::Tether;
use velcro::hash_map;

#[tokio::test]
async fn folder_expansion() {
    // Setup:
    //   * Setup User:
    //     + Create a Custom Folders not expanded
    //   * Create Sidebar
    let name = "foo";
    let ctx = MailTestContext::new().await;
    ctx.setup_user(sidebar_test_params(name, false)).await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();
    ctx.init_user(user_ctx.clone()).await;
    let sidebar = Sidebar::new(user_ctx.clone());

    let folder = get_folder("foo", &tether).await;
    assert!(!folder.expanded);

    ctx.mock_patch_label(folder.remote_id.unwrap(), true).await;
    ctx.catch_all().await;

    // Action
    sidebar
        .expand_folder(folder.local_id.unwrap())
        .await
        .unwrap();

    // Tests
    let folder = get_folder(name, &tether).await;
    assert!(folder.expanded);
}

#[tokio::test]
async fn folder_collapse() {
    // Setup:
    //   * Setup User:
    //     + Create a Custom Folders expanded
    //   * Create Sidebar
    let name = "foo";
    let ctx = MailTestContext::new().await;
    ctx.setup_user(sidebar_test_params(name, true)).await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();
    ctx.init_user(user_ctx.clone()).await;
    let sidebar = Sidebar::new(user_ctx.clone());

    let folder = get_folder("foo", &tether).await;
    assert!(folder.expanded);

    ctx.mock_patch_label(folder.remote_id.unwrap(), false).await;
    ctx.catch_all().await;

    // Action
    sidebar
        .collapse_folder(folder.local_id.unwrap())
        .await
        .unwrap();

    // Tests
    let folder = get_folder(name, &tether).await;
    assert!(!folder.expanded);
}

async fn get_folder(name: &str, tether: &Tether) -> Label {
    Label::find_first("WHERE remote_id = ?", params![name.to_owned()], tether)
        .await
        .unwrap()
        .unwrap()
}

fn sidebar_test_params(name: &str, state: bool) -> Params {
    TestParams {
        labels: hash_map! { LabelType::Folder: vec![ create_label(name, state) ]},
        ..Default::default()
    }
}

fn create_label(name: &str, expanded: bool) -> ApiLabel {
    ApiLabel {
        id: LabelId::from(name),
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
