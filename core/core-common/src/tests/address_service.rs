use proton_core_api::services::proton::AddressSignedKeyList;
use stash::orm::Model;
use stash::stash::StashError;

use crate::UserContext;
use crate::datatypes::{AddressFlags, AddressStatus, AddressType};
use crate::models::Address;
use crate::test_utils::account::{
    TEST_ADDRESS_KEY_SIGNATURE, test_api_address, testdata_address_keys_for_user_address,
};
use crate::test_utils::test_context::TestContext;

#[tokio::test]
async fn test_has_valid_sender_address_with_non_byoe_address() {
    let test_ctx = TestContext::new().await;
    let user_ctx = test_ctx.user_context().await;
    setup(&user_ctx, default_addresses()).await;

    let result = user_ctx.address_service().find_valid_sender_address().await;

    let address = result.unwrap().unwrap();
    assert_eq!(address.email, "hello@world");
}

#[tokio::test]
async fn test_has_valid_sender_address_with_only_byoe_addresses() {
    let test_ctx = TestContext::new().await;

    let addresses = vec![
        create_test_address("byoe1", "byoe1@example.com", AddressFlags::BYOE),
        create_test_address("byoe2", "byoe2@example.com", AddressFlags::BYOE),
    ];
    let user_ctx = test_ctx.user_context().await;
    setup(&user_ctx, addresses).await;

    let result = user_ctx.address_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_mixed_addresses() {
    let test_ctx = TestContext::new().await;

    let addresses = vec![
        create_test_address("byoe1", "byoe1@example.com", AddressFlags::BYOE),
        create_test_address("normal1", "normal1@proton.me", AddressFlags::default()),
    ];

    let user_ctx = test_ctx.user_context().await;
    setup(&user_ctx, addresses).await;

    let result = user_ctx.address_service().find_valid_sender_address().await;

    let address = result.unwrap().unwrap();
    assert_eq!(address.email, "normal1@proton.me");
}

#[tokio::test]
async fn test_has_valid_sender_address_disabled_addresses() {
    let test_ctx = TestContext::new().await;

    let addresses = vec![create_disabled_address(
        "disabled1",
        "disabled1@proton.me",
        AddressFlags::default(),
    )];

    let user_ctx = test_ctx.user_context().await;
    setup(&user_ctx, addresses).await;

    let result = user_ctx.address_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_no_send_permission() {
    let test_ctx = TestContext::new().await;

    let addresses = vec![create_no_send_address(
        "nosend1",
        "nosend1@proton.me",
        AddressFlags::default(),
    )];

    let user_ctx = test_ctx.user_context().await;
    setup(&user_ctx, addresses).await;

    let result = user_ctx.address_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_no_receive_permission() {
    let test_ctx = TestContext::new().await;

    let addresses = vec![create_no_receive_address(
        "noreceive1",
        "noreceive1@proton.me",
        AddressFlags::default(),
    )];

    let user_ctx = test_ctx.user_context().await;
    setup(&user_ctx, addresses).await;

    let result = user_ctx.address_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_null_flags() {
    let test_ctx = TestContext::new().await;

    let addresses = vec![create_test_address(
        "nullflags1",
        "nullflags1@proton.me",
        AddressFlags::default(),
    )];

    let user_ctx = test_ctx.user_context().await;
    setup(&user_ctx, addresses).await;

    let result = user_ctx.address_service().find_valid_sender_address().await;

    let address = result.unwrap().unwrap();
    assert_eq!(address.email, "nullflags1@proton.me");
}

#[tokio::test]
async fn test_has_valid_sender_address_empty_database() {
    let test_ctx = TestContext::new().await;

    let addresses = vec![];

    let user_ctx = test_ctx.user_context().await;
    setup(&user_ctx, addresses).await;

    let result = user_ctx.address_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_has_valid_sender_address_complex_scenario() {
    let test_ctx = TestContext::new().await;

    let addresses = vec![
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

    let user_ctx = test_ctx.user_context().await;
    setup(&user_ctx, addresses).await;

    let result = user_ctx.address_service().find_valid_sender_address().await;

    assert!(result.unwrap().is_none());
}

async fn setup(ctx: &UserContext, addresses: Vec<Address>) {
    let mut tether = ctx.stash().connection().await.unwrap();
    tether
        .tx(async move |tx| {
            for mut address in addresses {
                address.save(tx).await?;
            }
            Ok::<_, StashError>(())
        })
        .await
        .unwrap();
    tracing::warn!("Setup completed");
}

fn create_test_address(id: &str, email: &str, flags: AddressFlags) -> Address {
    Address {
        local_id: None,
        remote_id: Some(id.into()),
        email: email.to_owned(),
        send: true,
        receive: true,
        status: AddressStatus::Enabled,
        domain_id: None,
        address_type: AddressType::Original,
        display_name: String::new(),
        signature: TEST_ADDRESS_KEY_SIGNATURE.to_owned(),
        keys: testdata_address_keys_for_user_address().into(),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList::default().into(),
        flags: Some(flags),
        display_order: 0,
    }
}

fn create_disabled_address(id: &str, email: &str, flags: AddressFlags) -> Address {
    let mut addr = create_test_address(id, email, flags);
    addr.status = AddressStatus::Disabled;
    addr
}

fn create_no_send_address(id: &str, email: &str, flags: AddressFlags) -> Address {
    let mut addr = create_test_address(id, email, flags);
    addr.send = false;
    addr
}

fn create_no_receive_address(id: &str, email: &str, flags: AddressFlags) -> Address {
    let mut addr = create_test_address(id, email, flags);
    addr.receive = false;
    addr
}

fn default_addresses() -> Vec<Address> {
    vec![test_api_address().into()]
}
