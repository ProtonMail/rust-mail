use super::server::{Scheme, Server};
use futures::prelude::*;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Options for the runners.
#[derive(Debug)]
pub struct Args {
    /// The scheme to use.
    pub scheme: Scheme,

    /// Users to register.
    pub users: Vec<(String, String)>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            scheme: Scheme::HTTP,
            users: vec![],
        }
    }
}

impl Args {
    /// Set the scheme to use.
    #[must_use]
    pub fn scheme(mut self, scheme: Scheme) -> Self {
        self.scheme = scheme;
        self
    }

    /// Register a new user.
    #[must_use]
    pub fn user(mut self, name: &str, pass: &str) -> Self {
        self.users.push((name.to_string(), pass.to_string()));
        self
    }
}

/// Runs the given test as an async block, providing a local server.
///
/// # Panics
///
/// Panics if the runtime or local server cannot be created.
pub fn run<F: Future>(args: Args, test: impl FnOnce(Arc<Server>) -> F) -> F::Output {
    let Ok(runtime) = Runtime::new() else {
        panic!("unable to create runtime")
    };

    let Ok(server) = Server::new(runtime.handle(), &args.scheme) else {
        panic!("unable to create server")
    };

    runtime.block_on(async move {
        for (name, pass) in args.users {
            if let Err(e) = server.new_user(&name, &pass).await {
                panic!("server failed to create user: {e}")
            }
        }

        let res = test(server.clone()).await;

        match server.stop().await {
            Ok(()) => res,
            Err(e) => panic!("server exited with error: {e}"),
        }
    })
}
