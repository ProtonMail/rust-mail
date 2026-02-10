use crate::test_utils::test_context::TestContext;
use proton_core_api::services::proton::GetKeysAllResponse;
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_crypto_account::keys::{
    APIPublicAddressKeyGroup, APIPublicKey, APIPublicKeySource, KeyFlag, SKLDataJson, SKLSignature,
    SignedKeyList,
};
use serde::Serialize;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path, query_param},
};

pub const TEST_OTHER_USER_EMAIL: &str = "rust_test2@proton.black";
pub const TEST_OTHER_USER_ADDRESS_KEY: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----\nVersion: ProtonMail\n\nxjMEZkX+zBYJKwYBBAHaRw8BAQdAWduKPt5zBtc+9DqBQLoc3zlqIF8jOUPI\nsFwx9Jy2P9jNMXJ1c3RfdGVzdDJAcHJvdG9uLmJsYWNrIDxydXN0X3Rlc3Qy\nQHByb3Rvbi5ibGFjaz7CjAQQFgoAPgWCZkX+zAQLCQcICZBdyhjQnv7YbAMV\nCAoEFgACAQIZAQKbAwIeARYhBE/4qNMBuldcY8XfjF3KGNCe/thsAAB3BgEA\nh7CjEeXYnIkTHq8/r/Ez6BY/rULlzJ5AdQcAjRb9AdEBAOgC2cvq1iTTHIyI\nqaZbSw4BJaLL40Gak3qcrl7h0KkHwqgEEBYIAFoFAmZF/wkJENTSGgVRp7Ls\nFiEENhVDvw2lbaYMs9qU1NIaBVGnsuwsHFRlc3QgT3BlblBHUCBDQSA8dGVz\ndC1vcGVucGdwLWNhQHByb3Rvbi5tZT4FgwDtTgAAAJtWAPkBxJiQ3NSE9o5l\n+38bkvYvPf4vbjwXI+q35E00cX6/nAEApnu8EZsDjBUdGASjbJXav/QvTuSe\nb5cL31u+BkSpyAfOOARmRf7MEgorBgEEAZdVAQUBAQdA8mtOle2xn1hxJX+f\nuujODfm3DSpJNQ4i/3o2ND6UJngDAQgHwngEGBYKACoFgmZF/swJkF3KGNCe\n/thsApsMFiEET/io0wG6V1xjxd+MXcoY0J7+2GwAAFLgAP9PWtGDtDcebw2U\noD0wfFBaiv5ciHonMvExh9COaFoeQAEAs4CVpapPBM7TfVfztTEGi3fEthvh\nDwN8/F95ArvAIAo=\n=SEhN\n-----END PGP PUBLIC KEY BLOCK-----\n";
pub const TEST_OTHER_USER_SKL_DATA: &str = "[{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"4ff8a8d301ba575c63c5df8c5dca18d09efed86c\",\"SHA256Fingerprints\":[\"e63eca945a6690e50f97dcf9a38009a5f1b1279b6dc47754957532808be54ed1\",\"d1e16de4e7d0d79364fec17c329fb01e40c420b55876938060684178ff320493\"]}]";
pub const TEST_OTHER_USER_SKL_SIGNATURE: &str = "-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwqkEARYKAFsFgmZF/wgJkF3KGNCe/thsMxSAAAAAABEAGWNvbnRleHRAcHJv\ndG9uLmNoa2V5LXRyYW5zcGFyZW5jeS5rZXktbGlzdBYhBE/4qNMBuldcY8Xf\njF3KGNCe/thsAAAp4wD/T6pgbBqKYQ4UeVqj+qXcM5aw7lGU2/e+LCG8kjuR\nOQgBAPAUKI2Ojuw50EPWLV87olCh0TvSDAuZJ31+cyOOjpEE\n=cLF/\n-----END PGP SIGNATURE-----\n";

/// Returns a public address key response for [`TEST_OTHER_USER_EMAIL`].
#[must_use]
pub fn testdata_address_keys_other_user() -> GetKeysAllResponse {
    GetKeysAllResponse {
        address_keys: APIPublicAddressKeyGroup {
            keys: vec![APIPublicKey {
                source: APIPublicKeySource::Proton,
                flags: KeyFlag::from(3_u8),
                primary: true,
                public_key: TEST_OTHER_USER_ADDRESS_KEY.to_owned(),
            }],
            signed_key_list: Some(SignedKeyList {
                min_epoch_id: None,
                max_epoch_id: None,
                expected_min_epoch_id: Some(67),
                data: Some(SKLDataJson::from(TEST_OTHER_USER_SKL_DATA)),
                obsolescence_token: None,
                signature: Some(SKLSignature::from(TEST_OTHER_USER_SKL_SIGNATURE)),
                revision: 1,
            }),
        },
        catch_all_keys: None,
        is_proton: false,
        proton_mx: true,
        unverified_keys: None,
        warnings: Vec::default(),
    }
}

impl TestContext {
    /// Generates a mock response for retrieving another user's public address keys.
    ///
    /// This function creates a mock response to simulate the retrieval of
    /// public address keys for a specified user.
    ///
    pub async fn mock_get_keys_all(&self, email: &str, response: GetKeysAllResponse) {
        self.mock_get_keys_all_with_internal_param(email, None, response)
            .await;
    }

    /// Generates a mock response for retrieving another user's public address keys.
    ///
    /// This function creates a mock response to simulate the retrieval of
    /// public address keys for a specified user.
    ///
    #[function_name::named]
    pub async fn mock_get_keys_all_with_internal_param(
        &self,
        email: &str,
        internal_only: Option<bool>,
        response: GetKeysAllResponse,
    ) {
        let mut mock = Mock::given(method("GET"))
            .and(path("/api/core/v4/keys/all"))
            .and(query_param("Email", email));
        if let Some(value) = internal_only {
            mock = mock.and(query_param("InternalOnly", u8::from(value).to_string()));
        }
        mock.respond_with(ResponseTemplate::new(200).set_body_json(response))
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generates a mock failure response for retrieving another user's public address keys.
    ///
    #[function_name::named]
    pub async fn mock_get_keys_all_failure(
        &self,
        email: &str,
        internal_only: Option<bool>,
        response: ApiErrorInfo,
    ) {
        let mut mock = Mock::given(method("GET"))
            .and(path("/api/core/v4/keys/all"))
            .and(query_param("Email", email));
        if let Some(value) = internal_only {
            mock = mock.and(query_param("InternalOnly", u8::from(value).to_string()));
        }
        mock.respond_with(ResponseTemplate::new(422).set_body_json(response))
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_get_auth(&self, modulus_id: String, modulus: String) {
        // until the circular reference problem from proton-account is resolved,
        // we can't import it in proton-core-common so we have to redeclare it
        #[derive(Clone, Debug, Serialize)]
        #[serde(rename_all = "PascalCase")]
        pub struct GetAuthModulusResponse {
            pub modulus: String,

            #[serde(rename = "ModulusID")]
            pub modulus_id: String,
        }

        Mock::given(method("GET"))
            .and(path("/api/auth/v4/modulus"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetAuthModulusResponse {
                    modulus,
                    modulus_id,
                }),
            )
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}
