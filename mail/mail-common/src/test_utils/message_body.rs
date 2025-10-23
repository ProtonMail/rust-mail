//! Message body related state and test data
use crate::datatypes::SystemLabelId;
use crate::test_utils::init::Params as TestParams;
use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType, DelinquentState,
    Flags as ApiFlags, ProductUsedSpace as ApiProductUsedSpace, Role as ApiRole, User as ApiUser,
    UserMnemonicStatus as ApiUserMnemonicStatus, UserType as ApiUserType,
};
use proton_core_api::services::proton::{AddressId, LabelId, UserId};
use proton_crypto_account::keys::{
    ArmoredPrivateKey, EncryptedKeyToken, KeyTokenSignature, LocalAddressKey, LocalSignedKeyList,
    LocalUserKey, UnlockedAddressKeys,
};
use proton_crypto_account::salts::KeySalt;
use proton_crypto_inbox::proton_crypto::crypto::KeyGeneratorAlgorithm;
use proton_crypto_inbox::proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_inbox::proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, KeyFlag, KeyId, LockedKey, UserKeys as ApiUserKeys,
};
use proton_crypto_inbox::proton_crypto_account::salts::{Salt, Salts};
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageBody as ApiMessageBody,
    MessageFlags as ApiMessageFlags, MessageMetadata as ApiMessageMetadata,
    MessageSender as ApiMessageSender, MimeType as ApiMimeType, ViewMode as ApiViewMode,
};
use std::collections::HashMap;
use std::iter;

#[must_use]
pub fn message_body_test_params() -> crate::test_utils::init::Params {
    TestParams {
        user_info: Some(message_body_test_user_info()),
        addresses: message_body_test_addresses(),
        mail_settings: Some(message_body_test_mail_settings()),
        ..Default::default()
    }
}

pub const TEST_USER_ID: &str =
    "jctxnoKsvmlISYpOtESCWNC4tcFbddXmcQ6yyM94YP4tBngrw4O9IKf8jxSLThqZyqFlX972kKwQCPriEeh4qg==";
pub const TEST_USER_ADDRESS_ID: &str =
    "LGXtB3TbNifsW1elXtCp5zyysma52yRf8NZZ10pUQrJfp1QQCSoFTXcIVDCZJycme6KYHsxCE_xdneJ10dt_iA==";
pub const TEST_USER_KEY_ID: &str =
    "aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ==";

pub const TEST_USER_PASSWORD: &str = "password";

pub const TEST_MESSAGE_BODY_DECRYPTED: &str = r#"<div style="font-family: Arial, sans-serif; font-size: 14px; color: rgb(0, 0, 0); background-color: rgb(255, 255, 255);">This is a test body.</div>"#;

pub const TEST_MESSAGE_BODY_MIME_DECRYPTED: &str =
    "This is a mime message with two attachments.\n\n\n\n";

pub const TEST_MESSAGE_BODY_MIME_SIGNATURE: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----\r\n\r\nxjMEZf15lRYJKwYBBAHaRw8BAQdArPz06hKiOUYSVs6dbHpKSh63bW5/QyIFqRvJ\r\n5wOALJnNMkx1a2FzIEJ1cmtoYWx0ZXIgPGtleXRyYW5zcGFyZW5jeW1haWxlckBn\r\nbWFpbC5jb20+wo8EExYIADcWIQSNEf53FU6EMmZs43pG8PpwjTNiIAUCZf15lQUJ\r\nBaOagAIbAwQLCQgHBRUICQoLBRYCAwEAAAoJEEbw+nCNM2IgaX0BANKGrENgM7nb\r\npt5uORfaT5JLx695q1RgKDetm6bQhB1/AQDHvY3oha+eabN+yKcOWKlvvNpbbbYz\r\njunnrmfm7d+HDM44BGX9eZUSCisGAQQBl1UBBQEBB0Aq4KRFu4d/XmR2UEGjsXeW\r\nCWvvKUkzsCR/wRDn8E/lRQMBCAfCfgQYFggAJhYhBI0R/ncVToQyZmzjekbw+nCN\r\nM2IgBQJl/XmVBQkFo5qAAhsMAAoJEEbw+nCNM2IgEzcBAPqEmyOcnbzbsGJaZ5uF\r\nEA3OfGH7anEg2xEbfZ0jxAh0AP9nsO+JqQrVW5m3aGW4MRMFRjnC2DIHthThNQMw\r\n1bZpDQ==\r\n=ziuc\r\n-----END PGP PUBLIC KEY BLOCK-----\r\n";

#[must_use]
pub fn message_body_test_user_info() -> ApiUser {
    ApiUser {
        id: UserId::from(TEST_USER_ID),
        name: Some("rust_test".to_owned()),
        display_name: None,
        email: "rust_test@proton.ch".to_owned(),
        used_space: 0,
        max_space: 0,
        max_upload: 0,
        user_type: ApiUserType::Proton,
        create_time: 0,
        credit: 0,
        currency: "EUR".to_owned(),
        keys: ApiUserKeys(vec![message_body_test_user_key()]),
        product_used_space: ApiProductUsedSpace {
            calendar: 0,
            contact: 0,
            drive: 0,
            mail: 0,
            pass: 0,
        },
        to_migrate: false,
        mnemonic_status: ApiUserMnemonicStatus::Unknown,
        role: ApiRole::None,
        private: false,
        subscribed: 0,
        services: 0,
        delinquent: DelinquentState::Paid,
        flags: ApiFlags {
            protected: false,
            onboard_checklist_storage_granted: false,
            has_temporary_password: false,
            test_account: false,
            no_login: false,
            recovery_attempt: false,
            sso: false,
            no_proton_address: false,
            has_a_byoe_address: false,
        },
    }
}

fn message_body_test_user_key() -> LockedKey {
    LockedKey {
        id: KeyId::from("aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ=="),
        version: 3,
        private_key: ArmoredPrivateKey::from("-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZie3jRYJKwYBBAHaRw8BAQdAAp+4PE1Sf5V95XrIY/P2dUNk1TOojoEG\nLuuOzULTa1v+CQMINYn0u3DCV01gjT+Noe2HzLxwP2hieZC1aoGCxSrLn0fs\nLeShqv2pCPZ+SdrjXB5s5Rq7OP5Kr/2gN+0KS0yLGdyirFZWe6m5T8j20UQ5\n0M07bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp\nbF91c2VAZG9tYWluLnRsZD7CjAQQFgoAPgWCZie3jQQLCQcICZA4nKgbRZBl\nGQMVCAoEFgACAQIZAQKbAwIeARYhBOZJEArPLqrMMxX8fzicqBtFkGUZAADk\n/AD+LA6NW1K+Z3IT66/DEtjH0cmw6HNqxkBdT7kaL2o5pAMA/j9b4JCurWk/\n62MBM4I9RwXzSo8lmgPiYwPp4d/xgEsMx4sEZie3jRIKKwYBBAGXVQEFAQEH\nQHvLC7RWIDsorX5ZmYwjZbUhbXnEcO2sYt8OFaIh5KtHAwEIB/4JAwhKivkG\nshycUGA6wZtPR2HqO6+jvvSlRau/g2eZnWqhnvB4iIYTcD+CPpcPnWrrNgTz\nAU+kQ5sVrP6OiKKHIkUvHT5+MwelTbcpievGx2zGwngEGBYKACoFgmYnt40J\nkDicqBtFkGUZApsMFiEE5kkQCs8uqswzFfx/OJyoG0WQZRkAAJ6BAQDv4nBl\nNnj0W7XiAjiwRmVrY/sdybelB6j01p7UrcVAxQEAtEmT2cSIScVdWH1j3H9l\n0gGE7amH+cm6CjXOA7+Uwwc=\n=RGJ0\n-----END PGP PRIVATE KEY BLOCK-----\n".to_owned()),
        token: None,
        signature: None,
        activation: None,
        primary: true,
        active: true,
        flags: None,
        recovery_secret: None,
        recovery_secret_signature: None,
        address_forwarding_id: None,
    }
}

#[must_use]
pub fn message_body_test_addresses() -> Vec<ApiAddress> {
    vec![ApiAddress {
        id: AddressId::from(TEST_USER_ADDRESS_ID),
        email: "rust_test@proton.ch".to_owned(),
        send: true,
        receive: true,
        status: ApiAddressStatus::Enabled,
        domain_id: None,
        address_type: ApiAddressType::Original,
        order: 0,
        display_name: "rust_test".to_owned(),
        signature: String::new(),
        keys: ApiAddressKeys(
            vec![LockedKey {
                id: KeyId::from("gzKDANARz0i8OHhGuZV-oFfURju0I3XeW_hNn09g13dS_NJ57UbW420UAcWb-0s93xoav22O_jARq61FyL3guw=="),
                version: 3,
                private_key: ArmoredPrivateKey::from("-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZie3jRYJKwYBBAHaRw8BAQdA0lnAs/zJxwALYyLq9jnthTTJauaqwvLQ\nod3cCVOua+v+CQMIcWjkpeADcjxgwP+7tEc2sfM3J4oWV/p344AsSBiK442t\n5GmxcPBNuj7P82Mjfj10MfhzxIgDF39KW85vcrL4BRuDYq4uSUURFnZmiLFS\nx80vcnVzdF90ZXN0QHByb3Rvbi5ibGFjayA8cnVzdF90ZXN0QHByb3Rvbi5i\nbGFjaz7CjAQQFgoAPgWCZie3jQQLCQcICZDD5SnHczmG6wMVCAoEFgACAQIZ\nAQKbAwIeARYhBBGxOGij+OleubdsX8PlKcdzOYbrAABxyQEA53ij2BO8KHOi\nlmhaB9qeaNDnZhlvNazM9O87r2Cm03UA/jLgvtPQe+HgIDbguMFSeacvAKSG\n2A5jl6AAPWjifF4Jx4sEZie3jRIKKwYBBAGXVQEFAQEHQLJ401cWczKQigvx\njfQ5DxVXvA9p+HRuW16642Ybd99+AwEIB/4JAwjsnBN5czXnymCSAHHIugJH\nwwH1rvooZGeZ26QZ/UhsjQwXy1O5J66plmBD1Oe/uZG4Ed6ylw1VwROmW03q\nrRWwYeeVSN20YMavgbAZT7AVwngEGBYKACoFgmYnt40JkMPlKcdzOYbrApsM\nFiEEEbE4aKP46V65t2xfw+Upx3M5husAAPU7AQCMKF564vtdGCY/KIGqAhm2\nSNUnK5w6MkGKgrztbAhvngD/VK3t0WB8mUqXC3JoS2xC6rtyiyciAjQvuwWT\n2ePDxgI=\n=5IIS\n-----END PGP PRIVATE KEY BLOCK-----\n".to_owned()),
                token: Some(EncryptedKeyToken::from("-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DJ8rw1vR308gSAQdAwfey4aUSny0pDcCM0OykFF+KoquoUEuc5I48NYNn\nNkYwdMVXcHgrNAOVkSgBcCS5VxaRb3Lmo610XkQRnCyuadgvce4pRFqtx0+A\nNCNgn/Px0nEB+tPsQJL+EePQHgMZXhXmW3tS6/7jxzyCkuJVKdXHFNu3kTNU\nthAEwWkLUrQu280+De/2UEFq8oB6vjvUJiohremKSNp2Wr8fhL+XQubLoCtw\nln9Pw5EL3607i64Cs5f88Ew35GeKPQw/uUuCI8uB0A==\n=dj6J\n-----END PGP MESSAGE-----\n".to_owned())),
                signature: Some(KeyTokenSignature::from("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwnUEARYKACcFgmYnt8kJkDicqBtFkGUZFiEE5kkQCs8uqswzFfx/OJyoG0WQ\nZRkAACZ4AP49xBDsaIUR1IEJlMqTdwaSJ+02eXXpJANwT/mg2QNTJwD/fXhq\nojjc2LEMrebiFAl4GQgXxkUgnPuvpCyiB80C3A8=\n=KsBO\n-----END PGP SIGNATURE-----\n".to_owned())),
                activation: None,
                primary: true,
                active: true,
                flags: Some(KeyFlag::from(3_u32)),
                recovery_secret: None,
                recovery_secret_signature: None,
                address_forwarding_id: None,
            }]
        ),
        catch_all: false,
        proton_mx: true,
        signed_key_list: ApiAddressSignedKeyList {
            min_epoch_id: Some(3),
            max_epoch_id: Some(66),
            expected_min_epoch_id: None,
            data: Some("[{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"11b13868a3f8e95eb9b76c5fc3e529c7733986eb\",\"SHA256Fingerprints\":[\"f16446135c9380b623bb201a1409bcfd6cb5144fe463b45d08b51e9e335e39ad\",\"ffb76afa704c9a6808bf67009f3a4f0155becf34ff395e3be2e557960b9a4e1c\"]}]".to_owned()),
            obsolescence_token: None,
            signature: Some("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwqkEARYKAFsFgmYnt8kJkMPlKcdzOYbrMxSAAAAAABEAGWNvbnRleHRAcHJv\ndG9uLmNoa2V5LXRyYW5zcGFyZW5jeS5rZXktbGlzdBYhBBGxOGij+Oleubds\nX8PlKcdzOYbrAABnFwD+JukILCsHB7JxsMY4zP9EU8SGhu5/Gwx2aLod9GR1\nfucBANdiI900lTkhTRMHDof4aZ/8Ef5uV1pmQ/CFHQYTcj4P\n=QEZt\n-----END PGP SIGNATURE-----\n".to_owned()),
            revision: 1,
        },
    }]
}

#[must_use]
#[allow(clippy::field_reassign_with_default)]
pub fn message_body_test_mail_settings() -> ApiMailSettings {
    ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    }
}

/* User salts {
    "Code": 1000,
    "KeySalts": [
    {
        "ID": "aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ==",
        "KeySalt": "6bIzN4A8bOwmsiEuCPj74g=="
    },
    {
        "ID": "gzKDANARz0i8OHhGuZV-oFfURju0I3XeW_hNn09g13dS_NJ57UbW420UAcWb-0s93xoav22O_jARq61FyL3guw==",
        "KeySalt": null
    }
    ]
}*/

#[must_use]
pub fn message_body_test_message_simple() -> ApiMessage {
    ApiMessage {
        metadata: ApiMessageMetadata {
            id: MessageId::from("blkMQzCHplN2H_FNJ2GdMtRkmr3f9v_cFma64_Cmi8IPw3wx_lK-0ZEqA8cBfIf0PeVbY2P7oVQVwPup-h0syg==".to_owned()),
            conversation_id: ConversationId::from("0R5oYZX2jLkT9WYyNrGmdp6K1sYYDraeaE8FTeNSJZ7Znb1UPJqBfvx_Tqb4gyVnGUeiPo3o7vKolaUt6PmVuw==".to_owned()),
            order: 0,
            address_id: AddressId::from(TEST_USER_ADDRESS_ID),
            label_ids: vec![LabelId::inbox()],
            external_id: None,
            subject: "Mail with test body".to_owned(),
            sender: ApiMessageSender::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            flags: ApiMessageFlags::DKIM_FAIL,
            time: 1_715_863_508,
            size: 333,
            unread: false,
            is_replied: false,
            is_replied_all: false,
            is_forwarded: false,
            expiration_time: 0,
            snooze_time: 0,
            num_attachments: 0,
            attachments_metadata: vec![],
        },
        body: ApiMessageBody {
            header: String::new(),
            parsed_headers: HashMap::default(),
            body: "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DGS71hsmM2EQSAQdAYdJSo4eHIE7InFrOSN3+7nIRKfkcsCAb7aPI86nI\ny2owI0FLuN3IlbCoKsFFXfSbnTff3IePkr7xmhQmUYrVk0h50kwkEVyHnyPI\nm2nyqZXA0sCKAbKKQlcvjlJbsyUpJvsIwHuggwrQ+7htDauT4/SB9hScyAPj\nICxCGfzOaXjcf1fqevOMDqIWaSEQpOcMw2ocGP4I8OKgylBfuy9DT0/RhJSe\nrDo2uhlYqs0xmUdlHWPvGKEy4TKlUk2JSAr9U4+5l4J5iIK9O/TVrU+Tf7Ot\nRdEFfN+ERJQmVqXcfSkoImVm7oi0QfNP3ExZ94vlFyBFch/Ox5Oco5wbetr3\nL7KPGWiEmLYDI/xeFNC4AO4FD+MVUHjIYqzS/GABxwJQ7pCC8WJXUHKS6ZNR\nNf8RGKGL1O2cbKWSuULb7HwWRGljWezyr5rPLKK7DaHX3wj2qmdQRcSzsKEu\nOLjlB6jppMjP2r/CZSqC+XbefwczOZxkLJQiw6ujB4etdiDFiM+QifJfrp6f\nhtf7JGwpxPa/IbiL5OlKy7NYYs6JXNYU\n=AVU2\n-----END PGP MESSAGE-----\n".to_owned(),
            reply_to: Default::default(),
            mime_type: ApiMimeType::TextHtml,
            attachments: vec![],
            reply_tos: vec![],
        },
    }
}

#[must_use]
pub fn message_body_test_message_mime() -> ApiMessage {
    ApiMessage {
        metadata: ApiMessageMetadata {
            id: MessageId::from("sUrSuXEN_wQ9dPeKcwquBnOJXqr4Lsb9Y1iAo2AXi0Wj-z2T5pAf2iANsmvXJBZr-mLTXeGnkEb_S56SfEUHOQ==".to_owned()),
            conversation_id: ConversationId::from("sEaYBselvkhNF_KB4QK-aVYUrZYJnGJovDdSxQMQ8NUwsJUgHLtLwtQdeKBVEYZ33obagXEj36yDTTejiXhKGg==".to_owned()),
            address_id: AddressId::from(TEST_USER_ADDRESS_ID),
            attachments_metadata: vec![],
            bcc_list: vec![],
            cc_list: vec![],
            expiration_time: 0,
            external_id: None,
            flags: ApiMessageFlags::empty(),
            is_forwarded: false,
            is_replied: false,
            is_replied_all: false,
            label_ids: vec![],
            num_attachments: 0,
            order: 3629,
            sender: ApiMessageSender::default(),
            size: 2334,
            snooze_time: 0,
            subject: "Messsage with two attachments".to_string(),
            time: 1_715_864_547,
            to_list: vec![],
            unread: false,
        },
        body: ApiMessageBody {
            attachments: vec![],
            body: "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\n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\nGelSG3lI\n=fy7R\n-----END PGP MESSAGE-----\n".to_string(),
            reply_to: Default::default(),
            reply_tos: vec![],
            header: String::new(),
            mime_type: ApiMimeType::MultipartMixed,
            parsed_headers: HashMap::default(),
        },
    }
}

pub fn message_body_test_user_secret() -> UserKeySecret {
    let salts = Salts::new(iter::once(Salt {
        id: KeyId::from(TEST_USER_KEY_ID),
        key_salt: Some(KeySalt::from("6bIzN4A8bOwmsiEuCPj74g==".to_owned())),
    }));
    let locked_key = message_body_test_user_key();
    let srp_provider = new_srp_provider();
    salts
        .salt_for_key(&srp_provider, &locked_key.id, TEST_USER_PASSWORD.as_bytes())
        .map(UserKeySecret)
        .unwrap()
}

pub fn generate_new_api_address(
    address_id: AddressId,
    address_email: &str,
    key_id: &str,
) -> ApiAddress {
    let provider = new_pgp_provider();
    let key_secret = message_body_test_user_secret();
    let user_key_message = message_body_test_user_key();

    let local_user_key = LocalUserKey {
        private_key: user_key_message.private_key,
    };
    let unlocked_user_key = local_user_key
        .unlock_and_assign_key_id(&provider, KeyId(String::default()), &key_secret.0)
        .expect("unlock should succeed");

    let fresh_address_key = LocalAddressKey::generate(
        &provider,
        address_email,
        KeyGeneratorAlgorithm::default(),
        KeyFlag::default(),
        true,
        &unlocked_user_key,
    )
    .expect("ok");

    let unlocked = fresh_address_key
        .unlock_and_assign_key_id(&provider, KeyId(String::from(key_id)), &unlocked_user_key)
        .expect("unlock should not fail");

    let list = UnlockedAddressKeys::from(unlocked);
    let skl = LocalSignedKeyList::generate(&provider, &list).unwrap();

    let address_key = fresh_address_key.private_key;
    let address_key_token = fresh_address_key.token.unwrap();
    let address_key_signature = fresh_address_key.signature.unwrap();

    let skl_data = skl.data;
    let skl_signature = skl.signature;

    ApiAddress {
        id: address_id,
        email: address_email.to_owned(),
        send: true,
        receive: true,
        status: ApiAddressStatus::Enabled,
        domain_id: None,
        address_type: ApiAddressType::Original,
        order: 0,
        display_name: address_email.to_owned(),
        signature: String::new(),
        keys: ApiAddressKeys(vec![LockedKey {
            id: KeyId::from(key_id),
            version: 3,
            private_key: address_key,
            token: Some(address_key_token),
            signature: Some(address_key_signature),
            activation: None,
            primary: true,
            active: true,
            flags: Some(fresh_address_key.flags),
            recovery_secret: None,
            recovery_secret_signature: None,
            address_forwarding_id: None,
        }]),
        catch_all: false,
        proton_mx: true,
        signed_key_list: ApiAddressSignedKeyList {
            min_epoch_id: Some(3),
            max_epoch_id: Some(66),
            expected_min_epoch_id: None,
            data: Some(skl_data.0),
            obsolescence_token: None,
            signature: Some(skl_signature.0),
            revision: 1,
        },
    }
}
