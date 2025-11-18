use crate::datatypes::{
    AddressFlags, AddressKeys, AddressSignedKeyList, AddressStatus, AddressType,
};
use crate::models::Address;
use crate::tests::common::new_core_test_connection;
use proton_core_api::services::proton::AddressId;
use proton_crypto_account::keys::{
    AddressKeys as RealAddressKeys, ArmoredPrivateKey, EncryptedKeyToken, KeyId, KeyTokenSignature,
    LockedKey,
};
use stash::orm::Model;
use stash::params;
use stash::stash::StashError;

#[tokio::test]
async fn test_address_create() {
    let mut conn = new_core_test_connection().await.connection().await.unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        let mut address = create_test_address();
        address.save(tx).await.expect("failed to create address");
        let db_address = Address::load(address.id(), tx)
            .await
            .expect("failed to get address")
            .expect("should exist");
        assert_eq!(address, db_address);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_address_update() {
    let mut conn = new_core_test_connection().await.connection().await.unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        let mut address = create_test_address();
        address.save(tx).await.expect("failed to create address");
        let mut address2 = create_test_address_updated();
        address2.save(tx).await.expect("failed to create duplicate");
        let db_address = Address::load(address.id(), tx)
            .await
            .expect("failed to get address")
            .expect("should exist");
        assert_eq!(address, db_address);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_address_delete() {
    let mut conn = new_core_test_connection().await.connection().await.unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        let mut address = create_test_address();
        address.save(tx).await.expect("failed to create address");
        tx.execute(
            "DELETE FROM addresses WHERE remote_id=?",
            params![address.remote_id.clone()],
        )
        .await
        .expect("failed to delete address");
        let db_address = Address::load(address.id(), tx)
            .await
            .expect("failed to get address");
        assert_eq!(db_address, None);
        Ok(())
    })
    .await
    .unwrap();
}

fn create_test_address() -> Address {
    Address {
        local_id: None,
        remote_id: Some(AddressId::from("address_id")),
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
                private_key: ArmoredPrivateKey::from("SOME_PRIVATE_KEY".to_owned()),
                token: Some(EncryptedKeyToken::from("SOME_TOKEN_".to_owned())),
                signature: Some(KeyTokenSignature::from("SOME_SIGNATURE".to_owned())),
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
                private_key: ArmoredPrivateKey::from("SOME_PRIVATE_KEY2".to_owned()),
                token: Some(EncryptedKeyToken::from("SOME_TOKEN_2".to_owned())),
                signature: Some(KeyTokenSignature::from("SOME_SIGNATURE2".to_owned())),
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
                private_key: ArmoredPrivateKey::from("SOME_PRIVATE_KEY3".to_owned()),
                token: Some(EncryptedKeyToken::from("SOME_TOKEN_3".to_owned())),
                signature: Some(KeyTokenSignature::from("SOME_SIGNATURE3".to_owned())),
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
                private_key: ArmoredPrivateKey::from("SOME_PRIVATE_KEY4".to_owned()),
                token: Some(EncryptedKeyToken::from("SOME_TOKEN_4".to_owned())),
                signature: Some(KeyTokenSignature::from("SOME_SIGNATURE4".to_owned())),
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
        flags: Some(AddressFlags::default()),
    }
}

fn create_test_address_updated() -> Address {
    let old_address = create_test_address();
    Address {
        local_id: old_address.local_id,
        remote_id: Some(AddressId::from("address_id2")),
        email: "hello_bar@mail.com".into(),
        send: false,
        receive: true,
        status: AddressStatus::Enabled,
        domain_id: Some("SOME OTHER ID".into()),
        address_type: AddressType::Original,
        display_order: 0,
        display_name: "My Display Name".to_owned(),
        signature: "Some Signature".to_owned(),
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
        flags: Some(AddressFlags::default()),
    }
}
