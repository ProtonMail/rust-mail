use anyhow::Result;
use proton_core_api::session::Config;

pub fn new_api_config(app: Option<String>, env: Option<String>) -> Result<Config> {
    let mut cfg = Config::default();

    if let Some(app) = app {
        cfg.app_version = app;
    }

    if let Some(env) = env {
        cfg.env_id = env.parse()?;
    }

    Ok(cfg)
}
