use anyhow::Result;
use proton_core_common::datatypes::{ApiConfig, AppDetails};

pub fn new_api_config(app_details: AppDetails, env: Option<String>) -> Result<ApiConfig> {
    let mut cfg = ApiConfig {
        app_details,
        ..ApiConfig::default()
    };

    if let Some(env) = env {
        cfg.env_id = env.parse()?;
    }

    Ok(cfg)
}
