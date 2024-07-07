mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_core::auth::UserKeySecret;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType, Flags as ApiFlags,
    ProductUsedSpace as ApiProductUsedSpace, User as ApiUser,
    UserMnemonicStatus as ApiUserMnemonicStatus, UserType as ApiUserType,
};
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::response_data::{
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageFlags as ApiMessageFlags,
    MessageMetadata as ApiMessageMetadata, MimeType as ApiMimeType, ViewMode as ApiViewMode,
};
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_crypto_inbox::proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_inbox::proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, KeyFlag, KeyId, LockedKey, UserKeys as ApiUserKeys,
};
use proton_crypto_inbox::proton_crypto_account::salts::{Salt, Salts};
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Message;
use proton_mail_common::Mailbox;
use stash::orm::Model;
use std::iter;
use tempdir::TempDir;

#[tokio::test]
#[ignore]
async fn mailbox_message_body_simple() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = message_body_test_params();

    let message = message_body_test_message_simple();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message_metadata(vec![message.metadata.clone()], 1)
        .await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.catch_all().await;
    ctx.user_context()
        .await
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(ctx.user_context().await, LabelId::inbox())
        .await
        .unwrap();
    mailbox.sync(10).await.unwrap();

    // Resolve local id.
    let saved_message = Message::load_using(1, &ctx.user_context().await.stash().connection())
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(saved_message.remote_id, Some(message.metadata.id.into()));

    // Decrypt the message body.
    let tmp_dir = TempDir::new("cache").expect("failed to create temp dir");
    let pgp_provider = new_pgp_provider();
    let decrypted_body = saved_message
        .message_body(
            tmp_dir.path(),
            ctx.user_context()
                .await
                .unlocked_address_keys(&pgp_provider, &saved_message.remote_id.clone().unwrap())
                .await
                .unwrap(),
            pgp_provider,
            ctx.user_context().await.session().api(),
        )
        .await
        .unwrap();

    let expected = r#"<div style="font-family: Arial, sans-serif; font-size: 14px; color: rgb(0, 0, 0); background-color: rgb(255, 255, 255);">This is a test body.</div>"#;
    assert_eq!(decrypted_body.body, expected);
}

fn message_body_test_params() -> TestParams {
    TestParams {
        last_event_id: None,
        user_info: Some(message_body_test_user_info()),
        user_settings: None,
        addresses: message_body_test_addresses(),
        mail_settings: Some(message_body_test_mail_settings()),
        labels: Default::default(),
        conversations: vec![],
        attachments: vec![],
        conversation_count: vec![],
        message_count: vec![],
    }
}

const TEST_USER_ID: &str =
    "jctxnoKsvmlISYpOtESCWNC4tcFbddXmcQ6yyM94YP4tBngrw4O9IKf8jxSLThqZyqFlX972kKwQCPriEeh4qg==";
const TEST_USER_ADDRESS_ID: &str =
    "LGXtB3TbNifsW1elXtCp5zyysma52yRf8NZZ10pUQrJfp1QQCSoFTXcIVDCZJycme6KYHsxCE_xdneJ10dt_iA==";
const TEST_USER_KEY_ID: &str =
    "aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ==";

const TEST_USER_PASSWORD: &str = "password";

fn message_body_test_user_info() -> ApiUser {
    ApiUser {
        id: ApiRemoteId::from(TEST_USER_ID),
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
        role: 0,
        private: 0,
        subscribed: 0,
        services: 0,
        delinquent: 0,
        flags: ApiFlags {
            protected: false,
            onboard_checklist_storage_granted: false,
            has_temporary_password: false,
            test_account: false,
            no_login: false,
            recovery_attempt: false,
            sso: false,
            no_proton_address: false,
        },
    }
}

fn message_body_test_user_key() -> LockedKey {
    LockedKey  {
        id: KeyId::from("aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ=="),
        version: 3,
        private_key: "-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZie3jRYJKwYBBAHaRw8BAQdAAp+4PE1Sf5V95XrIY/P2dUNk1TOojoEG\nLuuOzULTa1v+CQMINYn0u3DCV01gjT+Noe2HzLxwP2hieZC1aoGCxSrLn0fs\nLeShqv2pCPZ+SdrjXB5s5Rq7OP5Kr/2gN+0KS0yLGdyirFZWe6m5T8j20UQ5\n0M07bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp\nbF91c2VAZG9tYWluLnRsZD7CjAQQFgoAPgWCZie3jQQLCQcICZA4nKgbRZBl\nGQMVCAoEFgACAQIZAQKbAwIeARYhBOZJEArPLqrMMxX8fzicqBtFkGUZAADk\n/AD+LA6NW1K+Z3IT66/DEtjH0cmw6HNqxkBdT7kaL2o5pAMA/j9b4JCurWk/\n62MBM4I9RwXzSo8lmgPiYwPp4d/xgEsMx4sEZie3jRIKKwYBBAGXVQEFAQEH\nQHvLC7RWIDsorX5ZmYwjZbUhbXnEcO2sYt8OFaIh5KtHAwEIB/4JAwhKivkG\nshycUGA6wZtPR2HqO6+jvvSlRau/g2eZnWqhnvB4iIYTcD+CPpcPnWrrNgTz\nAU+kQ5sVrP6OiKKHIkUvHT5+MwelTbcpievGx2zGwngEGBYKACoFgmYnt40J\nkDicqBtFkGUZApsMFiEE5kkQCs8uqswzFfx/OJyoG0WQZRkAAJ6BAQDv4nBl\nNnj0W7XiAjiwRmVrY/sdybelB6j01p7UrcVAxQEAtEmT2cSIScVdWH1j3H9l\n0gGE7amH+cm6CjXOA7+Uwwc=\n=RGJ0\n-----END PGP PRIVATE KEY BLOCK-----\n".to_owned(),
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

fn message_body_test_addresses() -> Vec<ApiAddress> {
    vec![ApiAddress {
       id: ApiRemoteId::from(TEST_USER_ADDRESS_ID),
       email: "rust_test@proton.ch".to_owned(),
       send: true,
       receive: true,
       status: ApiAddressStatus::Enabled,
       domain_id: None,
       address_type: ApiAddressType::Original,
       order: 0,
       display_name: "rust_test".to_owned(),
       signature: "".to_owned(),
       keys: ApiAddressKeys(
           vec![LockedKey{
               id: KeyId::from("gzKDANARz0i8OHhGuZV-oFfURju0I3XeW_hNn09g13dS_NJ57UbW420UAcWb-0s93xoav22O_jARq61FyL3guw=="),
               version: 3,
               private_key: "-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZie3jRYJKwYBBAHaRw8BAQdA0lnAs/zJxwALYyLq9jnthTTJauaqwvLQ\nod3cCVOua+v+CQMIcWjkpeADcjxgwP+7tEc2sfM3J4oWV/p344AsSBiK442t\n5GmxcPBNuj7P82Mjfj10MfhzxIgDF39KW85vcrL4BRuDYq4uSUURFnZmiLFS\nx80vcnVzdF90ZXN0QHByb3Rvbi5ibGFjayA8cnVzdF90ZXN0QHByb3Rvbi5i\nbGFjaz7CjAQQFgoAPgWCZie3jQQLCQcICZDD5SnHczmG6wMVCAoEFgACAQIZ\nAQKbAwIeARYhBBGxOGij+OleubdsX8PlKcdzOYbrAABxyQEA53ij2BO8KHOi\nlmhaB9qeaNDnZhlvNazM9O87r2Cm03UA/jLgvtPQe+HgIDbguMFSeacvAKSG\n2A5jl6AAPWjifF4Jx4sEZie3jRIKKwYBBAGXVQEFAQEHQLJ401cWczKQigvx\njfQ5DxVXvA9p+HRuW16642Ybd99+AwEIB/4JAwjsnBN5czXnymCSAHHIugJH\nwwH1rvooZGeZ26QZ/UhsjQwXy1O5J66plmBD1Oe/uZG4Ed6ylw1VwROmW03q\nrRWwYeeVSN20YMavgbAZT7AVwngEGBYKACoFgmYnt40JkMPlKcdzOYbrApsM\nFiEEEbE4aKP46V65t2xfw+Upx3M5husAAPU7AQCMKF564vtdGCY/KIGqAhm2\nSNUnK5w6MkGKgrztbAhvngD/VK3t0WB8mUqXC3JoS2xC6rtyiyciAjQvuwWT\n2ePDxgI=\n=5IIS\n-----END PGP PRIVATE KEY BLOCK-----\n".to_owned(),
               token: Some("-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DJ8rw1vR308gSAQdAwfey4aUSny0pDcCM0OykFF+KoquoUEuc5I48NYNn\nNkYwdMVXcHgrNAOVkSgBcCS5VxaRb3Lmo610XkQRnCyuadgvce4pRFqtx0+A\nNCNgn/Px0nEB+tPsQJL+EePQHgMZXhXmW3tS6/7jxzyCkuJVKdXHFNu3kTNU\nthAEwWkLUrQu280+De/2UEFq8oB6vjvUJiohremKSNp2Wr8fhL+XQubLoCtw\nln9Pw5EL3607i64Cs5f88Ew35GeKPQw/uUuCI8uB0A==\n=dj6J\n-----END PGP MESSAGE-----\n".to_owned()),
               signature: Some("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwnUEARYKACcFgmYnt8kJkDicqBtFkGUZFiEE5kkQCs8uqswzFfx/OJyoG0WQ\nZRkAACZ4AP49xBDsaIUR1IEJlMqTdwaSJ+02eXXpJANwT/mg2QNTJwD/fXhq\nojjc2LEMrebiFAl4GQgXxkUgnPuvpCyiB80C3A8=\n=KsBO\n-----END PGP SIGNATURE-----\n".to_owned()),
               activation: None,
               primary: true,
               active: true,
               flags: Some(KeyFlag::from(3u32)),
               recovery_secret: None,
               recovery_secret_signature: None,
               address_forwarding_id: None,
           }]
       ),
       catch_all: false,
       proton_mx: true,
       signed_key_list: ApiAddressSignedKeyList{
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

fn message_body_test_mail_settings() -> ApiMailSettings {
    let mut settings = ApiMailSettings::default();
    settings.view_mode = ApiViewMode::Messages;
    settings
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

fn message_body_test_message_simple() -> ApiMessage {
    ApiMessage {
       metadata: ApiMessageMetadata {
           id: ApiRemoteId::from("blkMQzCHplN2H_FNJ2GdMtRkmr3f9v_cFma64_Cmi8IPw3wx_lK-0ZEqA8cBfIf0PeVbY2P7oVQVwPup-h0syg==".to_owned()),
           conversation_id: ApiRemoteId::from("0R5oYZX2jLkT9WYyNrGmdp6K1sYYDraeaE8FTeNSJZ7Znb1UPJqBfvx_Tqb4gyVnGUeiPo3o7vKolaUt6PmVuw==".to_owned()),
           order: 0,
           address_id: ApiRemoteId::from(TEST_USER_ADDRESS_ID),
           label_ids: vec![LabelId::inbox().into()],
           external_id: None,
           subject: "Mail with test body".to_owned(),
           sender: Default::default(),
           to_list: vec![],
           cc_list: vec![],
           bcc_list: vec![],
           reply_tos: vec![],
           flags: ApiMessageFlags::DKIM_FAIL,
           time:  1715863508,
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
       header: String::new(),
       parsed_headers: Default::default(),
       body: "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DGS71hsmM2EQSAQdAYdJSo4eHIE7InFrOSN3+7nIRKfkcsCAb7aPI86nI\ny2owI0FLuN3IlbCoKsFFXfSbnTff3IePkr7xmhQmUYrVk0h50kwkEVyHnyPI\nm2nyqZXA0sCKAbKKQlcvjlJbsyUpJvsIwHuggwrQ+7htDauT4/SB9hScyAPj\nICxCGfzOaXjcf1fqevOMDqIWaSEQpOcMw2ocGP4I8OKgylBfuy9DT0/RhJSe\nrDo2uhlYqs0xmUdlHWPvGKEy4TKlUk2JSAr9U4+5l4J5iIK9O/TVrU+Tf7Ot\nRdEFfN+ERJQmVqXcfSkoImVm7oi0QfNP3ExZ94vlFyBFch/Ox5Oco5wbetr3\nL7KPGWiEmLYDI/xeFNC4AO4FD+MVUHjIYqzS/GABxwJQ7pCC8WJXUHKS6ZNR\nNf8RGKGL1O2cbKWSuULb7HwWRGljWezyr5rPLKK7DaHX3wj2qmdQRcSzsKEu\nOLjlB6jppMjP2r/CZSqC+XbefwczOZxkLJQiw6ujB4etdiDFiM+QifJfrp6f\nhtf7JGwpxPa/IbiL5OlKy7NYYs6JXNYU\n=AVU2\n-----END PGP MESSAGE-----\n".to_owned(),
       mime_type: ApiMimeType::TextHtml,
       attachments: vec![],
   }
}

pub fn message_body_test_user_secret() -> UserKeySecret {
    let salts = Salts::new(iter::once(Salt {
        id: KeyId::from(TEST_USER_KEY_ID),
        key_salt: Some("6bIzN4A8bOwmsiEuCPj74g==".to_owned()),
    }));
    let locked_key = message_body_test_user_key();
    let srp_provider = new_srp_provider();
    salts
        .salt_for_key(&srp_provider, &locked_key.id, TEST_USER_PASSWORD.as_bytes())
        .map(UserKeySecret)
        .unwrap()
}
