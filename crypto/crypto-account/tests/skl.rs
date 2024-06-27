use proton_crypto::{
    crypto::{DataEncoding, PGPProviderSync},
    new_pgp_provider,
};

use proton_crypto_account::keys::{
    APIPublicAddressKeyGroup, APIPublicKey, APIPublicKeySource, DecryptedAddressKey,
    DecryptedUserKey, KeyFlag, KeyId, LocalSignedKeyList, PublicAddressKeyGroup, SKLDataJson,
    SKLSignature, SignedKeyList, UnlockedAddressKey, UnlockedUserKey,
};

const TEST_USER_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZie3jRYJKwYBBAHaRw8BAQdAAp+4PE1Sf5V95XrIY/P2dUNk1TOojoEG
LuuOzULTa1v+CQMIyRkIEctTq7tgD0cHCXSlb9RoIlW0FkasmPMPfJW3+ejY
Vk8849vYU4NIXwz8F6i/2i3hmVgUWQ+BwjBPubFEphf+U6TMIft6crssukxa
1807bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp
bF91c2VAZG9tYWluLnRsZD7CjAQQFgoAPgWCZie3jQQLCQcICZA4nKgbRZBl
GQMVCAoEFgACAQIZAQKbAwIeARYhBOZJEArPLqrMMxX8fzicqBtFkGUZAADk
/AD+LA6NW1K+Z3IT66/DEtjH0cmw6HNqxkBdT7kaL2o5pAMA/j9b4JCurWk/
62MBM4I9RwXzSo8lmgPiYwPp4d/xgEsMx4sEZie3jRIKKwYBBAGXVQEFAQEH
QHvLC7RWIDsorX5ZmYwjZbUhbXnEcO2sYt8OFaIh5KtHAwEIB/4JAwjMoZVn
MffCPWDpnZRakUprlUVlvDHHjPCJw7zVbFKfvTvYqqxsnNcqC74crMs7WVkE
WI6Thna3/aBfMLkC7t9RnHE6u4wFZMF/SJ+ZomLtwngEGBYKACoFgmYnt40J
kDicqBtFkGUZApsMFiEE5kkQCs8uqswzFfx/OJyoG0WQZRkAAJ6BAQDv4nBl
Nnj0W7XiAjiwRmVrY/sdybelB6j01p7UrcVAxQEAtEmT2cSIScVdWH1j3H9l
0gGE7amH+cm6CjXOA7+Uwwc=
=OHv0
-----END PGP PRIVATE KEY BLOCK-----";

const TEST_ADDRESS_KEY_PRIVATE: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZie3jRYJKwYBBAHaRw8BAQdA0lnAs/zJxwALYyLq9jnthTTJauaqwvLQ
od3cCVOua+v+CQMIhZwosaEFrVdgBWld0b0A2Buch+gTvX4AigO5y8rRErRs
c1KjMyUGUmU5qmJpR3lmX7lWFK9zHZV+u7ybTBX/9fvkFcVLugS2z+I0dE3K
Ds0vcnVzdF90ZXN0QHByb3Rvbi5ibGFjayA8cnVzdF90ZXN0QHByb3Rvbi5i
bGFjaz7CjAQQFgoAPgWCZie3jQQLCQcICZDD5SnHczmG6wMVCAoEFgACAQIZ
AQKbAwIeARYhBBGxOGij+OleubdsX8PlKcdzOYbrAABxyQEA53ij2BO8KHOi
lmhaB9qeaNDnZhlvNazM9O87r2Cm03UA/jLgvtPQe+HgIDbguMFSeacvAKSG
2A5jl6AAPWjifF4Jx4sEZie3jRIKKwYBBAGXVQEFAQEHQLJ401cWczKQigvx
jfQ5DxVXvA9p+HRuW16642Ybd99+AwEIB/4JAwiZPoLcohue0mAid4zMemsH
gvqoauEgGIdKuWpBcLT7PQFkgzxlbveHdKwVDjiAhDPE4RV2LZ36QKbDVqhk
/5rwCfSqircDl8fDO/RPzUKBwngEGBYKACoFgmYnt40JkMPlKcdzOYbrApsM
FiEEEbE4aKP46V65t2xfw+Upx3M5husAAPU7AQCMKF564vtdGCY/KIGqAhm2
SNUnK5w6MkGKgrztbAhvngD/VK3t0WB8mUqXC3JoS2xC6rtyiyciAjQvuwWT
2ePDxgI=
=bOcf
-----END PGP PRIVATE KEY BLOCK-----
";

fn get_test_keys<T: PGPProviderSync>(provider: &T) -> PublicAddressKeyGroup<T::PublicKey> {
    let api_address_keys = APIPublicAddressKeyGroup{
        keys: vec![
            APIPublicKey{
                source:APIPublicKeySource::Proton,
                flags: KeyFlag::from(3_u32),
                public_key: "-----BEGIN PGP PUBLIC KEY BLOCK-----\nVersion: ProtonMail\n\nxjMEYV78vBYJKwYBBAHaRw8BAQdATzuHJEfffnkkxR6voPu8hMI30ZleJZrF\nci81cphX+9jNL3Rlc3RrdEBrdC5wcm90b24uYmxhY2sgPHRlc3RrdEBrdC5w\ncm90b24uYmxhY2s+wo8EEBYKACAFAmFe/LwGCwkHCAMCBBUICgIEFgIBAAIZ\nAQIbAwIeAQAhCRAk1S96jDVEmRYhBAkUoLQ3MQGw5M9DtyTVL3qMNUSZo6UB\nAPzhGjHv//jl43mqXEo2/V47nREbm9MofSMOh+nqfg6wAP94opkrY95h9WVu\nG5+63MJWeHfVChrtYGLdE5PuSeSBBc44BGFe/LwSCisGAQQBl1UBBQEBB0B8\nQ43HsvkQ2JimHPujgpIcwDyMAnVxjoYJWHiDyZ9yKgMBCAfCeAQYFggACQUC\nYV78vAIbDAAhCRAk1S96jDVEmRYhBAkUoLQ3MQGw5M9DtyTVL3qMNUSZ9kkA\n/jzoeQgc7VnhdliB5VvOk7dKQBI4kqGpK7at8ThZHPXYAP9g7k0OjUeMfnh/\nNP1i3leIoG0QRT9lJ4XM0qcrhVqjBg==\n=XfZT\n-----END PGP PUBLIC KEY BLOCK-----\n".to_owned() 
            },
        ],
        signed_key_list: None
    };
    api_address_keys.import(provider).unwrap()
}

fn get_test_skl() -> SignedKeyList {
    SignedKeyList {
        min_epoch_id: Some(32),
        max_epoch_id: Some(35),
        data: Some(SKLDataJson::from("[{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"0914a0b4373101b0e4cf43b724d52f7a8c354499\",\"SHA256Fingerprints\":[\"99dfe8acfa4e091fb81c88dca947cf05fc2e764332cd20484ddea016f3ef1c35\",\"ac0f9568fc061b980cb02fb5b29471460f065353a2375c6f30fc4465c903f26e\"]},{\"Primary\":0,\"Flags\":3,\"Fingerprint\":\"59f1af56b673645834574e705ffcf74f485dc81a\",\"SHA256Fingerprints\":[\"8ccca8278be421752a659eadffd0b1d3a7a0cf778d97254b2cea2f4ab7faef93\",\"9e967080956e786bb098423853062b9c26193de48fb83107df8332828d0973c3\"]},{\"Primary\":0,\"Flags\":3,\"Fingerprint\":\"68ec39a2f0c0bf87c1a3ee6c03301a8551e6040c\",\"SHA256Fingerprints\":[\"57d5299ce1d187f1b606b6a7f45d8b21a4154fcd94e87d39cdd60aa11207129a\",\"490afb29e10416fcac60889a3e8841b7063eb4eeb6f843abf6ffc83d14d7c1ea\"]}]")),
        signature: Some(SKLSignature("-----BEGIN PGP SIGNATURE-----\r\nVersion: OpenPGP.js v4.10.10\r\nComment: https://openpgpjs.org\r\n\r\nwnUEARYKAAYFAmFfBeIAIQkQJNUveow1RJkWIQQJFKC0NzEBsOTPQ7ck1S96\r\njDVEmWQ1AQC1mZcKKhL9Ub9gX/HI6s3QeCG40zKG57g64BhmcNM2dAD/UhZv\r\nT2eWnpQ5JeboHlSsw1m+RRGwtqQ+u4al9F6o7Ac=\r\n=CiSs\r\n-----END PGP SIGNATURE-----\r\n".to_owned())),
        expected_min_epoch_id: None,
        obsolescence_token: None,
        revision: 1,
    }
}

fn create_test_private_keys_with_skl<Provider: PGPProviderSync>(
    provider: &Provider,
) -> (
    UnlockedUserKey<Provider>,
    UnlockedAddressKey<Provider>,
    SignedKeyList,
) {
    let private_key = provider
        .private_key_import(TEST_USER_KEY, "password", DataEncoding::Armor)
        .unwrap();
    let public_key = provider.private_key_to_public_key(&private_key).unwrap();
    let user_key = DecryptedUserKey {
        id: KeyId::from("aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ=="),
        private_key,
        public_key,
    };
    let private_key = provider
        .private_key_import(TEST_ADDRESS_KEY_PRIVATE, "password", DataEncoding::Armor)
        .unwrap();
    let public_key = provider.private_key_to_public_key(&private_key).unwrap();
    let key = DecryptedAddressKey {
        id: KeyId::from("gzKDANARz0i8OHhGuZV-oFfURju0I3XeW_hNn09g13dS_NJ57UbW420UAcWb-0s93xoav22O_jARq61FyL3guw=="),
        flags: KeyFlag::from(3_u32),
        primary: true,
        private_key,
        public_key,
    };
    let skl = SignedKeyList {
        min_epoch_id: Some(3),
        max_epoch_id: Some(283),
        data: Some(SKLDataJson::from("[{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"11b13868a3f8e95eb9b76c5fc3e529c7733986eb\",\"SHA256Fingerprints\":[\"f16446135c9380b623bb201a1409bcfd6cb5144fe463b45d08b51e9e335e39ad\",\"ffb76afa704c9a6808bf67009f3a4f0155becf34ff395e3be2e557960b9a4e1c\"]}]")),
        signature: Some(SKLSignature::from("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwqkEARYKAFsFgmYnt8kJkMPlKcdzOYbrMxSAAAAAABEAGWNvbnRleHRAcHJv\ndG9uLmNoa2V5LXRyYW5zcGFyZW5jeS5rZXktbGlzdBYhBBGxOGij+Oleubds\nX8PlKcdzOYbrAABnFwD+JukILCsHB7JxsMY4zP9EU8SGhu5/Gwx2aLod9GR1\nfucBANdiI900lTkhTRMHDof4aZ/8Ef5uV1pmQ/CFHQYTcj4P\n=QEZt\n-----END PGP SIGNATURE-----\n")),
        expected_min_epoch_id: None,
        obsolescence_token: None,
        revision: 1,
    };
    (user_key, key, skl)
}

#[test]
fn test_retrieve_skl_data() {
    let skl = get_test_skl();
    let skl_data_result = skl.signed_key_list_data();
    assert!(skl_data_result.is_ok());
}

#[test]
fn test_verify_skl_data() {
    let provider = new_pgp_provider();
    let skl = get_test_skl();
    let public_keys = get_test_keys(&provider);
    skl.verify_signature(&provider, public_keys.as_ref(), None)
        .unwrap();
}

#[test]
fn test_create_skl_data() {
    let provider = new_pgp_provider();
    let (user_key, key, skl) = create_test_private_keys_with_skl(&provider);
    let local_skl = LocalSignedKeyList::generate(&provider, &user_key, &[key])
        .expect("SKL generation must not fail");
    assert_eq!(&local_skl.data, skl.data.as_ref().unwrap());
    let dummy_skl = SignedKeyList {
        min_epoch_id: None,
        max_epoch_id: None,
        expected_min_epoch_id: None,
        data: Some(local_skl.data),
        obsolescence_token: None,
        signature: Some(local_skl.signature),
        revision: 1,
    };
    dummy_skl
        .verify_signature(&provider, &[&user_key.public_key], None)
        .expect("signature should verify");
}
