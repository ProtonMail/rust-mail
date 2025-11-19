use crate::test_utils::init::Params;
use crate::test_utils::test_context::MailTestContext;
use proton_core_api::services::proton::AddressSignedKeyList;
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressId, AddressStatus as ApiAddressStatus,
    AddressType as ApiAddressType,
};
use proton_core_common::datatypes::AddressFlags;
use proton_core_common::test_utils::account::{
    TEST_ADDRESS_KEY_SIGNATURE, testdata_address_keys_for_user_address,
};

fn create_test_address(id: &str, email: &str, flags: AddressFlags) -> ApiAddress {
    ApiAddress {
        id: AddressId::from(id),
        email: email.to_owned(),
        send: true,
        receive: true,
        status: ApiAddressStatus::Enabled,
        domain_id: None,
        address_type: ApiAddressType::Original,
        order: 0,
        display_name: String::new(),
        signature: TEST_ADDRESS_KEY_SIGNATURE.to_owned(),
        keys: testdata_address_keys_for_user_address(),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList::default(),
        flags: flags.into(),
    }
}

fn create_disabled_address(id: &str, email: &str, flags: AddressFlags) -> ApiAddress {
    let mut addr = create_test_address(id, email, flags);
    addr.status = ApiAddressStatus::Disabled;
    addr
}

fn create_no_send_address(id: &str, email: &str, flags: AddressFlags) -> ApiAddress {
    let mut addr = create_test_address(id, email, flags);
    addr.send = false;
    addr
}

fn create_no_receive_address(id: &str, email: &str, flags: AddressFlags) -> ApiAddress {
    let mut addr = create_test_address(id, email, flags);
    addr.receive = false;
    addr
}

#[tokio::test]
async fn test_has_valid_sender_address_with_non_byoe_address() {
    let test_ctx = MailTestContext::new().await;
    test_ctx.setup_user(Params::default_basic()).await;
    test_ctx.catch_all().await;
    let user_ctx = test_ctx.mail_user_context().await;

    let result = user_ctx.account_service().find_valid_sender_address().await;

    let address = result.unwrap().unwrap();
    assert_eq!(address.email, "rust_test@proton.black");
}

#[tokio::test]
async fn test_has_valid_sender_address_with_only_byoe_addresses() {
    let test_ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.addresses = vec![
        create_test_address("byoe1", "byoe1@example.com", AddressFlags::BYOE),
        create_test_address("byoe2", "byoe2@example.com", AddressFlags::BYOE),
    ];

    test_ctx.setup_user(params).await;
    test_ctx.catch_all().await;
    let user_ctx = test_ctx.mail_user_context().await;

    let result = user_ctx.account_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_mixed_addresses() {
    let test_ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.addresses = vec![
        create_test_address("byoe1", "byoe1@example.com", AddressFlags::BYOE),
        create_test_address("normal1", "normal1@proton.me", AddressFlags::default()),
    ];

    test_ctx.setup_user(params).await;
    test_ctx.catch_all().await;
    let user_ctx = test_ctx.mail_user_context().await;

    let result = user_ctx.account_service().find_valid_sender_address().await;

    let address = result.unwrap().unwrap();
    assert_eq!(address.email, "normal1@proton.me");
}

#[tokio::test]
async fn test_has_valid_sender_address_disabled_addresses() {
    let test_ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.addresses = vec![create_disabled_address(
        "disabled1",
        "disabled1@proton.me",
        AddressFlags::default(),
    )];

    test_ctx.setup_user(params).await;
    test_ctx.catch_all().await;
    let user_ctx = test_ctx.mail_user_context().await;

    let result = user_ctx.account_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_no_send_permission() {
    let test_ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.addresses = vec![create_no_send_address(
        "nosend1",
        "nosend1@proton.me",
        AddressFlags::default(),
    )];

    test_ctx.setup_user(params).await;
    test_ctx.catch_all().await;
    let user_ctx = test_ctx.mail_user_context().await;

    let result = user_ctx.account_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_no_receive_permission() {
    let test_ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.addresses = vec![create_no_receive_address(
        "noreceive1",
        "noreceive1@proton.me",
        AddressFlags::default(),
    )];

    test_ctx.setup_user(params).await;
    test_ctx.catch_all().await;
    let user_ctx = test_ctx.mail_user_context().await;

    let result = user_ctx.account_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_null_flags() {
    let test_ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.addresses = vec![create_test_address(
        "nullflags1",
        "nullflags1@proton.me",
        AddressFlags::default(),
    )];

    test_ctx.setup_user(params).await;
    test_ctx.catch_all().await;
    let user_ctx = test_ctx.mail_user_context().await;

    let result = user_ctx.account_service().find_valid_sender_address().await;

    let address = result.unwrap().unwrap();
    assert_eq!(address.email, "nullflags1@proton.me");
}

#[tokio::test]
async fn test_has_valid_sender_address_empty_database() {
    let test_ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.addresses = vec![];

    test_ctx.setup_user(params).await;
    test_ctx.catch_all().await;
    let user_ctx = test_ctx.mail_user_context().await;

    let result = user_ctx.account_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_complex_scenario() {
    let test_ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.addresses = vec![
        create_disabled_address(
            "disabled_byoe",
            "disabled_byoe@example.com",
            AddressFlags::BYOE,
        ),
        create_no_send_address(
            "enabled_byoe_no_send",
            "enabled_byoe_no_send@example.com",
            AddressFlags::BYOE,
        ),
        create_test_address(
            "enabled_byoe_valid",
            "enabled_byoe_valid@example.com",
            AddressFlags::BYOE,
        ),
    ];

    test_ctx.setup_user(params).await;
    test_ctx.catch_all().await;
    let user_ctx = test_ctx.mail_user_context().await;

    let result = user_ctx.account_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}
