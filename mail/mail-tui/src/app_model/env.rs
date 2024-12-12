use proton_mail_common::proton_api_mail::proton_api_core::services::proton::Config;

#[derive(Copy, Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum Env {
    #[default]
    /// Production environment (proton.me)
    Prod,

    /// Development environment (proton.black)
    Dev,
}

impl Env {
    pub fn api_config(self) -> Config {
        match self {
            Env::Prod => Config::default(),
            Env::Dev => Config {
                base_url: "https://mail.proton.black/api/".to_owned(),
                ..Default::default()
            },
        }
    }
}
