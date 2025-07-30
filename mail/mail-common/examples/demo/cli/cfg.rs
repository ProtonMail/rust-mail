use anyhow::Result;
use proton_core_api::session::Config;
use proton_core_common::datatypes::AppDetails;

pub fn new_api_config(app: &AppDetails, env: Option<String>) -> Result<Config> {
    let mut cfg = Config {
        app_version: app.format_api_app_version(),
        ..Config::default()
    };

    if let Some(env) = env {
        cfg.env_id = env.parse()?;
    }

    Ok(cfg)
}
