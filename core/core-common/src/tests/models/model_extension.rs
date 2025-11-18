use crate::{
    datatypes::{AddressFlags, AddressKeys, AddressSignedKeyList, AddressStatus, AddressType},
    models::Address,
    tests::common::new_core_test_connection,
};
use proton_core_api::services::proton::AddressId;
use proton_crypto_account::keys::AddressKeys as RealAddressKeys;
use stash::stash::StashError;
use stash::{orm::Model, params};

#[tokio::test]
async fn count_test() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            for i in 0..10 {
                let mut address = create_test_address(i);
                address.save(tx).await.expect("failed to create address");

                assert_eq!(Address::count("", vec![], tx).await.unwrap(), i as u64 + 1);
            }
            Ok(())
        })
        .await
        .unwrap();

    assert_eq!(
        Address::count("WHERE remote_id = ?", params!["address_id_1"], &tether)
            .await
            .unwrap(),
        1
    );
}

fn create_test_address(id: usize) -> Address {
    Address {
        local_id: None,
        remote_id: Some(AddressId::from(format!("address_id_{id}"))),
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
        flags: Some(AddressFlags::default()),
    }
}
