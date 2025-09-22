use muon::client::{Builder, Fingerprint};
use muon::test::store::TestStore;
use muon::{App, Client};
use serde_json::json;

const USER: &str = "plus";
const PASS: &str = "plus";

const USER_2FA: &str = "twofa";
const PASS_2FA: &str = "a";
const TOTP_2FA: &str = "4R5YJICSS6N72KNN3YRTEGLJCEKIMSKJ";

/// Simple ping tests against the Atlas API.
mod ping;

/// Auth tests.
mod auth;

/// Error tests.
mod error;

/// Timeout tests.
mod timeout;

/// Mail tests.
mod mail;

/// Runtime tests.
mod runtime;

/// Parallel tests.
mod parallel;

/// TLS tests.
mod tls;

/// Creates a new test client.
fn new_client() -> Client {
    new_builder().build().expect("client should build")
}

/// Creates a new test client builder.
fn new_builder() -> Builder {
    let app = App::new("android-mail@99.9.40.0-dev")
        .unwrap()
        .with_user_agent("ProtonMail/99.9.40.0-dev (Android 15; google sdk_gphone64_arm64)");
    let store = new_atlas_store();

    Client::builder(app, store)
}

/// Create a new test store for the Atlas environment.
fn new_atlas_store() -> TestStore {
    if let Ok(name) = std::env::var("ENV_NAME") {
        TestStore::atlas_name(&name)
    } else {
        TestStore::atlas()
    }
}

/// Creates a valid fingerprint
fn valid_fingerprint() -> Fingerprint {
    json!({
        "mail-android-99.9.40.0-challenge":{
            "appLang":"en",
            "deviceName":"TestDevice",
            "frame":{
                "name":"username"
            },
            "isDarkmodeOn":false,
            "isJailbreak":false,
            "keyboards":[

            ],
            "preferredContentSize":"2.0",
            "regionCode":"CH",
            "storageCapacity":"63.8",
            "timezone":"Europe/Zurich",
            "timezoneOffset":"0",
            "v":"2.0.0"
        }
    })
    .into()
}

/// Creates an invalid fingerprint
fn invalid_fingerprint() -> Fingerprint {
    json!(100).into()
}
