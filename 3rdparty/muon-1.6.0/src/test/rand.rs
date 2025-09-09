use crate::common::Host;
use crate::deps::url::Url;
use rand::distributions::{Alphanumeric, DistString};

/// Generate a random URL that is very very *very* unlikely to exist.
#[must_use]
pub fn random_url() -> Url {
    url!("https://{}", random_domain()).unwrap()
}

/// Generate a random domain name.
#[must_use]
pub fn random_domain() -> String {
    format!("{}.xyz", random_string(32))
}

/// Generate a random host.
#[must_use]
pub fn random_host() -> Host {
    Host::direct(random_domain()).unwrap()
}

/// Generate a random alphanumeric of the given length.
#[must_use]
pub fn random_string(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), len)
}
