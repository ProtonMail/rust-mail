use crate::db::{
    new_core_test_connection, CoreSqliteConnection, CoreSqliteConnectionMut, DBResult,
};
use proton_api_core::domain::{
    Address, AddressId, AddressSignedKeyList, AddressStatus, AddressType,
};
use proton_api_core::exports::crypto::domain::{AddressKeys, KeyId, LockedKey};

pub(crate) fn with_tx(conn: &mut CoreSqliteConnection, f: impl Fn(&mut CoreSqliteConnectionMut)) {
    conn.tx(move |tx| -> DBResult<()> {
        (f)(tx);
        Ok(())
    })
    .expect("failed transaction");
}

#[test]
fn test_address_create() {
    let mut conn = new_core_test_connection();
    with_tx(&mut conn, |tx| {
        let address = create_test_address();
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
    let mut conn = new_core_test_connection();
    with_tx(&mut conn, |tx| {
        let mut address = create_test_address();
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
    let mut conn = new_core_test_connection();
    with_tx(&mut conn, |tx| {
        let address = create_test_address();
        tx.create_or_update_address(&address)
            .expect("failed to create address");
        let address = create_test_address_updated();
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
    let mut conn = new_core_test_connection();
    with_tx(&mut conn, |tx| {
        let address = create_test_address();
        tx.create_or_update_address(&address)
            .expect("failed to create address");
        tx.delete_address(&address.id)
            .expect("failed to delete address");
        let db_address = tx.get_address(&address.id).expect("failed to get address");
        assert_eq!(db_address, None);
    });
}

fn create_test_address() -> Address {
    Address {
        id: AddressId::from("address_id"),
        email: "hello@mail.com".into(),
        send: true,
        receive: false,
        status: AddressStatus::Enabled,
        domain_id: Some("id".into()),
        address_type: AddressType::Original,
        order: 0,
        display_name: String::new(),
        signature: String::new(),
        keys: AddressKeys(vec![
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
        ]),
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

fn create_test_address_updated() -> Address {
    let old_address = create_test_address();
    Address {
        id: AddressId::from("address_id"),
        email: "hello_bar@mail.com".into(),
        send: false,
        receive: true,
        status: AddressStatus::Enabled,
        domain_id: Some("SOME OTHER ID".into()),
        address_type: AddressType::Original,
        order: 0,
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
    }
}
