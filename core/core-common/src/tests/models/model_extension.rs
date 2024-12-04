use proton_crypto_account::keys::AddressKeys as RealAddressKeys;
use stash::params;

use crate::{
    datatypes::{AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, RemoteId},
    models::{Address, ModelExtension},
    tests::common::new_core_test_connection,
};

#[tokio::test]
async fn count_test() {
    let stash = new_core_test_connection().await;
    for i in 0..10 {
        let mut address = create_test_address(i);
        address
            .save(&stash)
            .await
            .expect("failed to create address");

        assert_eq!(
            Address::count("", vec![], &stash).await.unwrap(),
            i as u64 + 1
        );
    }

    assert_eq!(
        Address::count("WHERE remote_id = ?", params!["address_id_1"], &stash)
            .await
            .unwrap(),
        1
    );
}

fn create_test_address(id: usize) -> Address {
    Address {
        local_id: None,
        remote_id: Some(RemoteId::from(format!("address_id_{id}"))),
        email: format!("hello_{id}@mail.com"),
        send: true,
        receive: false,
        status: AddressStatus::Enabled,
        domain_id: Some("id".into()),
        address_type: AddressType::Original,
        display_order: 0,
        display_name: String::new(),
        signature: String::new(),
        keys: AddressKeys(RealAddressKeys(vec![])),
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
    }
}
