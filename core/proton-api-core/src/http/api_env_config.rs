use super::{DEFAULT_APP_VERSION, DEFAULT_HOST_URL};

/// API Environment Configuration
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone)]
pub struct APIEnvConfig {
    pub app_version: String,
    pub base_url: String,
    pub user_agent: String,
    pub allow_http: bool,
    pub skip_srp_proof_validation: bool,
}

impl Default for APIEnvConfig {
    fn default() -> Self {
        APIEnvConfig {
            app_version: DEFAULT_APP_VERSION.to_string(),
            user_agent: "NoClient/0.1.0".to_string(),
            base_url: DEFAULT_HOST_URL.to_string(),
            allow_http: false,
            skip_srp_proof_validation: false,
        }
    }
}
