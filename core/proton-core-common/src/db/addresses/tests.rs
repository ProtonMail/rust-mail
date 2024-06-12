use crate::db::new_core_test_connection;
use proton_api_core::domain::addresses::AddressKeys;
use proton_api_core::domain::{
    Address, AddressId, AddressSignedKeyList, AddressStatus, AddressType,
};
use proton_api_core::exports::crypto::keys::{AddressKeys as RealAddressKeys, KeyId, LockedKey};
use stash::orm::Model;
use stash::params;
use stash::stash::Stash;

#[tokio::test]
async fn test_address_create() {
    let conn = new_core_test_connection().await;
    let tx = conn
        .transaction()
        .await
        .expect("Failed to start transaction");
    let mut address = create_test_address(&conn);
    address.save().await.expect("failed to create address");
    let db_address = Address::load_using(address.id.clone(), &tx)
        .await
        .expect("failed to get address")
        .expect("should exist");
    assert_eq!(address, db_address);
    tx.commit().await.expect("Failed to commit transaction");
}

#[tokio::test]
async fn test_address_create_duplicate() {
    let conn = new_core_test_connection().await;
    let tx = conn
        .transaction()
        .await
        .expect("Failed to start transaction");
    let mut address = create_test_address(&conn);
    address.save().await.expect("failed to create address");
    let mut address2 = create_test_address(&conn);
    address2.display_order = 10;
    assert!(address2.save().await.is_err());
    let db_address = Address::load_using(address.id.clone(), &tx)
        .await
        .expect("failed to get address")
        .expect("should exist");
    assert_eq!(address, db_address);
    tx.commit().await.expect("Failed to commit transaction");
}

#[tokio::test]
async fn test_address_update() {
    let conn = new_core_test_connection().await;
    let tx = conn
        .transaction()
        .await
        .expect("Failed to start transaction");
    let mut address = create_test_address(&conn);
    address.save().await.expect("failed to create address");
    let mut address2 = create_test_address_updated(&conn);
    address2.save().await.expect("failed to create duplicate");
    let db_address = Address::load_using(address.id.clone(), &tx)
        .await
        .expect("failed to get address")
        .expect("should exist");
    assert_eq!(address, db_address);
    tx.commit().await.expect("Failed to commit transaction");
}

#[tokio::test]
async fn test_address_delete() {
    let conn = new_core_test_connection().await;
    let tx = conn
        .transaction()
        .await
        .expect("Failed to start transaction");
    let mut address = create_test_address(&conn);
    address.save().await.expect("failed to create address");
    tx.execute(
        "DELETE FROM addresses WHERE id=?",
        params![address.id.clone()],
    )
    .await
    .expect("failed to delete address");
    let db_address = Address::load_using(address.id.clone(), &tx)
        .await
        .expect("failed to get address");
    assert_eq!(db_address, None);
    tx.commit().await.expect("Failed to commit transaction");
}

fn create_test_address(stash: &Stash) -> Address {
    Address {
        id: AddressId::from("address_id"),
        email: "hello@mail.com".into(),
        send: true,
        receive: false,
        status: AddressStatus::Enabled,
        domain_id: Some("id".into()),
        address_type: AddressType::Original,
        display_order: 0,
        display_name: String::new(),
        signature: String::new(),
        keys: AddressKeys(RealAddressKeys(vec![
            LockedKey {
                id: KeyId::from("key_id"),
                version: 0,
                private_key: "SOME_PRIVATE_KEY".to_string(),
                token: Some("SOME_TOKEN_".to_string()),
                signature: Some("SOME_SIGNATURE".to_string()),
                activation: None,
                primary: true,
                active: true,
                flags: None,
                recovery_secret: None,
                recovery_secret_signature: None,
                address_forwarding_id: None,
            },
            LockedKey {
                id: KeyId::from("key_id2"),
                version: 0,
                private_key: "SOME_PRIVATE_KEY2".to_string(),
                token: Some("SOME_TOKEN_2".to_string()),
                signature: Some("SOME_SIGNATURE2".to_string()),
                activation: None,
                primary: true,
                active: true,
                flags: None,
                recovery_secret: None,
                recovery_secret_signature: None,
                address_forwarding_id: None,
            },
            LockedKey {
                id: KeyId::from("key_id3"),
                version: 0,
                private_key: "SOME_PRIVATE_KEY3".to_string(),
                token: Some("SOME_TOKEN_3".to_string()),
                signature: Some("SOME_SIGNATURE3".to_string()),
                activation: None,
                primary: true,
                active: true,
                flags: None,
                recovery_secret: None,
                recovery_secret_signature: None,
                address_forwarding_id: None,
            },
            LockedKey {
                id: KeyId::from("key_id4"),
                version: 0,
                private_key: "SOME_PRIVATE_KEY4".to_string(),
                token: Some("SOME_TOKEN_4".to_string()),
                signature: Some("SOME_SIGNATURE4".to_string()),
                activation: None,
                primary: true,
                active: true,
                flags: None,
                recovery_secret: None,
                recovery_secret_signature: None,
                address_forwarding_id: None,
            },
        ])),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList {
            min_epoch_id: None,
            max_epoch_id: None,
            expected_min_epoch_id: None,
            data: None,
            obsolescence_token: None,
            signature: None,
            revision: 20,
        },
        row_id: None,
        stash: Some(stash.clone()),
    }
}

fn create_test_address_updated(stash: &Stash) -> Address {
    let old_address = create_test_address(stash);
    Address {
        id: AddressId::from("address_id2"),
        email: "hello_bar@mail.com".into(),
        send: false,
        receive: true,
        status: AddressStatus::Enabled,
        domain_id: Some("SOME OTHER ID".into()),
        address_type: AddressType::Original,
        display_order: 0,
        display_name: "My Display Name".to_string(),
        signature: "Some Signature".to_string(),
        keys: old_address.keys.clone(),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList {
            min_epoch_id: None,
            max_epoch_id: None,
            expected_min_epoch_id: None,
            data: None,
            obsolescence_token: None,
            signature: None,
            revision: 20,
        },
        row_id: None,
        stash: Some(stash.clone()),
    }
}
