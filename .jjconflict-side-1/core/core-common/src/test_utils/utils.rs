use rand::{Rng, distributions::Uniform};
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::{MockServer, Respond, ResponseTemplate};

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

/// Set up mock endpoints for auth sessions and token refresh.
///
/// This should be called for any `MockServer` that will handle Session creation
/// to ensure the muon client's automatic auth session and token refresh requests
/// are properly mocked.
pub async fn mock_auth_endpoints(mock_server: &MockServer) {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    let session_response = serde_json::json!({
        "ServerProof": "dummy",
        "UID": "dummy",
        "AccessToken": "dummy",
        "RefreshToken": "dummy",
        "Scopes": ["dummy"],
        "2FA": { "Enabled": 0 },
        "PasswordMode": 1,
    });

    Mock::given(method("POST"))
        .and(path_regex(r".*/auth/v4/sessions$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_response))
        .mount(mock_server)
        .await;

    let refresh_response = serde_json::json!({
        "UID": "dummy",
        "AccessToken": "dummy",
        "RefreshToken": "dummy",
        "Scopes": ["dummy"],
    });

    Mock::given(method("POST"))
        .and(path_regex(r".*/auth/v4/refresh$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(refresh_response))
        .mount(mock_server)
        .await;
}

/// Whenever we need to test a specific response pattern.
/// Example: Service is unavailable for the first 3 times.
pub struct RespondNthTime {
    count: AtomicUsize,
    max: usize,
    before: ResponseTemplate,
    after: ResponseTemplate,
}

impl RespondNthTime {
    #[must_use]
    pub fn new(max: usize, before: ResponseTemplate, after: ResponseTemplate) -> Self {
        Self {
            count: AtomicUsize::new(0),
            max,
            before,
            after,
        }
    }
}
impl Respond for RespondNthTime {
    fn respond(&self, _request: &wiremock::Request) -> ResponseTemplate {
        let time = self.count.fetch_add(1, Ordering::SeqCst);
        if time < self.max {
            return self.before.clone();
        }

        self.after.clone()
    }
}
