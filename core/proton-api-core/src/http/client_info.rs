use super::APIEnvConfig;

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub env_config: APIEnvConfig,
    pub debug: bool,
}
