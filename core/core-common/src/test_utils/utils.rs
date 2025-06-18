use rand::{Rng, distributions::Uniform};
use wiremock::{Mock, MockServer, Request, matchers::any};
/// Generates a random string of the specified length, including alphanumeric and special characters.
///
#[must_use]
pub fn random_string(length: usize) -> String {
    let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                           abcdefghijklmnopqrstuvwxyz\
                           0123456789!@#$%^&*()_+-=[]{}|;:'\",.<>?/\\`~";

    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.sample(Uniform::new(0, charset.len()));
            charset[idx] as char
        })
        .collect()
}

/// Set up a catch-all mock for the mock server.
///
/// Calls to this function need to come at the END of the test setup, AFTER
/// all other mocks have been set up. This will ensure that any unconfigured
/// calls will cause the test to fail.
///
/// It is unfortunately not possible to use the [`Mock::with_priority()`]
/// method to set this up by default as a lower-priority expectation and
/// establish a catch-all in that way.
///
pub async fn catch_all(mock_server: &MockServer) {
    // If there are any unconfigured calls, we will panic because it's not what
    // we expect to happen, so the test should fail
    Mock::given(any())
        .respond_with(|request: &Request| {
            panic!(
                "Received unexpected {} request\n  Path: {}\n  Headers:\n{}\n  Body: {}\n",
                request.method,
                request.url.path(),
                request
                    .headers
                    .iter()
                    .map(|header| format!("    {}: {:?}", header.0, header.1))
                    .collect::<Vec<String>>()
                    .join("\n"),
                String::from_utf8(request.body.clone()).unwrap(),
            );
        })
        .named("Catch all mock")
        .mount(mock_server)
        .await;
}
