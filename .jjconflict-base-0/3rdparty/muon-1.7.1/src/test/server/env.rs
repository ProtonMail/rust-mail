use crate::app::AppVersion;
use crate::common::{Host, Server};
use crate::env::Env;
use crate::tls::TlsPinSet;
use std::collections::HashMap;

/// A custom environment.
#[derive(Debug, Clone)]
pub struct TestEnv(HashMap<Server, Option<TlsPinSet>>);

impl TestEnv {
    /// Create a new test environment.
    pub fn new(servers: impl IntoIterator<Item = (Server, Option<TlsPinSet>)>) -> Self {
        Self(servers.into_iter().collect())
    }
}

impl Env for TestEnv {
    fn servers(&self, _: &AppVersion) -> Vec<Server> {
        self.0.keys().cloned().collect()
    }

    fn pins(&self, host: &Host) -> Option<&TlsPinSet> {
        self.0.iter().find_map(|(server, pins)| {
            if server.host() == host {
                pins.as_ref()
            } else {
                None
            }
        })
    }
}
