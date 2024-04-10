use crate::domain::SecretString;
use secrecy::ExposeSecret;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Protocol {
    Https,
    Socks5,
}

#[derive(Debug, Clone)]
pub struct Auth {
    pub username: String,
    pub password: SecretString,
}

#[derive(Debug, Clone)]
pub struct Proxy {
    pub protocol: Protocol,
    pub auth: Option<Auth>,
    pub url: String,
    pub port: u16,
}

impl Proxy {
    #[must_use]
    pub fn as_url(&self) -> String {
        let protocol = match self.protocol {
            Protocol::Https => "https",
            Protocol::Socks5 => "socks5",
        };

        let auth = if let Some(auth) = &self.auth {
            format!("{}:{}@", auth.username, auth.password.expose_secret())
        } else {
            String::new()
        };

        format!("{protocol}://{auth}{}:{}", self.url, self.port)
    }
}
