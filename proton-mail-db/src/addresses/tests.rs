use crate::{new_test_connection, with_tx};
use proton_api_mail::domain::{
    Address, AddressId, AddressSignedKeyList, AddressStatus, AddressType,
};
use proton_api_mail::exports::crypto::domain::{KeyId, LockedKey};
use proton_api_mail::proton_api_core::domain::ProtonBoolean;
use proton_api_mail::proton_api_core::exports::crypto::domain::AddressKeys;

#[test]
fn test_address_create() {
    let (mut conn, _, _t) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let address = creat_test_address();
        tx.create_or_update_address(&address)
            .expect("failed to create address");
        let db_address = tx
            .get_address(&address.id)
            .expect("failed to get address")
            .expect("should exist");
        assert_eq!(address, db_address);
    });
}

#[test]
fn test_address_create_duplicate() {
    let (mut conn, _, _t) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let mut address = creat_test_address();
        tx.create_or_update_address(&address)
            .expect("failed to create address");
        address.order = 10;
        tx.create_or_update_address(&address)
            .expect("failed to create duplicate");
        let db_address = tx
            .get_address(&address.id)
            .expect("failed to get address")
            .expect("should exist");
        assert_eq!(address, db_address);
    });
}

#[test]
fn test_address_update() {
    let (mut conn, _, _t) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let address = creat_test_address();
        tx.create_or_update_address(&address)
            .expect("failed to create address");
        let address = creat_test_address_updated();
        tx.update_address(&address)
            .expect("failed to create duplicate");
        let db_address = tx
            .get_address(&address.id)
            .expect("failed to get address")
            .expect("should exist");
        assert_eq!(address, db_address);
    });
}

#[test]
fn test_address_delete() {
    let (mut conn, _, _t) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let address = creat_test_address();
        tx.create_or_update_address(&address)
            .expect("failed to create address");
        tx.delete_address(&address.id)
            .expect("failed to delete address");
        let db_address = tx.get_address(&address.id).expect("failed to get address");
        assert_eq!(db_address, None);
    });
}

fn creat_test_address() -> Address {
    Address {
        id: AddressId::from("address_id"),
        email: "hello@mail.com".into(),
        send: ProtonBoolean::True,
        receive: ProtonBoolean::False,
        status: AddressStatus::Enabled,
        domain_id: Some("id".into()),
        address_type: AddressType::Original,
        order: 0,
        display_name: "".to_string(),
        signature: "".to_string(),
        keys: AddressKeys(vec![LockedKey {
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
        }]),
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
    }
}

fn creat_test_address_updated() -> Address {
    Address {
        id: AddressId::from("address_id"),
        email: "hello_bar@mail.com".into(),
        send: ProtonBoolean::False,
        receive: ProtonBoolean::True,
        status: AddressStatus::Enabled,
        domain_id: Some("SOME OTHER ID".into()),
        address_type: AddressType::Original,
        order: 0,
        display_name: "My Display Name".to_string(),
        signature: "Some Signature".to_string(),
        keys: AddressKeys(vec![LockedKey {
            id: KeyId::from("key_id"),
            version: 0,
            private_key: "SOME_PRIVATE_KEY_2".to_string(),
            token: Some("SOME_TOKEN_2".to_string()),
            signature: Some("SOME_SIGNATURE2".to_string()),
            activation: None,
            primary: true,
            active: true,
            flags: None,
            recovery_secret: None,
            recovery_secret_signature: None,
            address_forwarding_id: None,
        }]),
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
    }
}
