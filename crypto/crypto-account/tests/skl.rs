use proton_crypto::{crypto::PGPProviderSync, new_pgp_provider};

use proton_crypto_account::keys::{
    APIPublicAddressKeyGroup, APIPublicKey, APIPublicKeySource, KeyFlag, PublicAddressKeyGroup,
    SKLSignature, SignedKeyList,
};

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
        data: Some("[{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"0914a0b4373101b0e4cf43b724d52f7a8c354499\",\"SHA256Fingerprints\":[\"99dfe8acfa4e091fb81c88dca947cf05fc2e764332cd20484ddea016f3ef1c35\",\"ac0f9568fc061b980cb02fb5b29471460f065353a2375c6f30fc4465c903f26e\"]},{\"Primary\":0,\"Flags\":3,\"Fingerprint\":\"59f1af56b673645834574e705ffcf74f485dc81a\",\"SHA256Fingerprints\":[\"8ccca8278be421752a659eadffd0b1d3a7a0cf778d97254b2cea2f4ab7faef93\",\"9e967080956e786bb098423853062b9c26193de48fb83107df8332828d0973c3\"]},{\"Primary\":0,\"Flags\":3,\"Fingerprint\":\"68ec39a2f0c0bf87c1a3ee6c03301a8551e6040c\",\"SHA256Fingerprints\":[\"57d5299ce1d187f1b606b6a7f45d8b21a4154fcd94e87d39cdd60aa11207129a\",\"490afb29e10416fcac60889a3e8841b7063eb4eeb6f843abf6ffc83d14d7c1ea\"]}]".to_owned()),
        signature: Some(SKLSignature("-----BEGIN PGP SIGNATURE-----\r\nVersion: OpenPGP.js v4.10.10\r\nComment: https://openpgpjs.org\r\n\r\nwnUEARYKAAYFAmFfBeIAIQkQJNUveow1RJkWIQQJFKC0NzEBsOTPQ7ck1S96\r\njDVEmWQ1AQC1mZcKKhL9Ub9gX/HI6s3QeCG40zKG57g64BhmcNM2dAD/UhZv\r\nT2eWnpQ5JeboHlSsw1m+RRGwtqQ+u4al9F6o7Ac=\r\n=CiSs\r\n-----END PGP SIGNATURE-----\r\n".to_owned())),
        expected_min_epoch_id: None,
        obsolescence_token: None,
        revision: 1,
    }
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
