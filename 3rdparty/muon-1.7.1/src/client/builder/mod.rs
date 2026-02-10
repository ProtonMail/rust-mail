use super::ClientInternalStorage;
use crate::common::prelude::*;
use crate::env::DynEnv;
use crate::headers::{AppVersionHeader, UserAgentHeader};
use crate::http::prelude::*;
use crate::middleware::{OnSendRetryHandler, Status429Handler, Status5xxHandler};
use crate::{App, Client, Result, Sealed};
use std::collections::VecDeque;

if_wasm! {{
    // On `wasm` targets, we use a `reqwest`-based connector.
    export! { mod reqwest (as pub); }

    /// The default builder on `wasm` targets uses `reqwest`.
    pub type Builder = ReqwestBuilder;
} else {
    // On non-`wasm` targets, we use our own `hyper`-based connector.
    export! { mod hyper (as pub); }

    /// The default builder on non-`wasm` targets uses `hyper`.
    pub type Builder = HyperBuilder;
}}

/// A type that can build an HTTP connector.
///
/// This enables a common builder pattern for configuring clients with different
/// underlying transports. This trait is sealed and cannot be implemented
/// outside of this crate.
pub trait Transport: Sealed + Default {
    /// Builds an HTTP connector for the given `env`.
    fn build(self, env: &DynEnv) -> Result<DynHttpConnector>;
}

/// A builder.
#[derive(Debug)]
pub struct BaseBuilder<T> {
    // --- Environment ---
    app: App,
    env: DynEnv,

    stores: ClientInternalStorage,

    // --- Config ---
    layers: Layers,
    policies: Policies,

    // --- Transport ---
    inner: T,
}

#[derive(Debug, Default)]
struct Layers {
    front: VecDeque<DynHttpSenderLayer>,
    back: VecDeque<DynHttpSenderLayer>,
}

#[derive(Debug, Default)]
struct Policies {
    retry: RetryPolicy,
}

impl<T: Transport> BaseBuilder<T> {
    pub(crate) fn new(app: App, stores: ClientInternalStorage) -> Self {
        let env = stores.env().to_owned().build();

        Self {
            app,
            env,
            stores,

            layers: Layers::default(),
            policies: Policies::default(),
            inner: T::default(),
        }
    }

    /// Adds a layer to the front of the stack.
    ///
    /// This layer will be the first to process the request and the last to
    /// process the response.
    pub fn layer_front(mut self, layer: impl IntoDyn<DynHttpSenderLayer>) -> Self {
        self.layers.front.push_front(layer.into_dyn());
        self
    }

    /// Adds a layer to the back of the stack.
    ///
    /// This layer will be the last to process the request and the first to
    /// process the response.
    pub fn layer_back(mut self, layer: impl IntoDyn<DynHttpSenderLayer>) -> Self {
        self.layers.back.push_back(layer.into_dyn());
        self
    }

    /// Sets the retry policy.
    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.policies.retry = policy;
        self
    }

    /// Builds the client.
    pub fn build(self) -> Result<Client> {
        // Build the base sender.
        let sender = HttpSender::new(
            self.inner.build(&self.env)?,
            self.env.servers(self.app.app_version()),
        );

        // Add the user's back layers.
        let sender = sender.layer(self.layers.back);

        // Add the default handlers.
        let sender = sender.layer([
            OnSendRetryHandler.into_dyn(),
            Status5xxHandler.into_dyn(),
            Status429Handler.into_dyn(),
        ]);

        // Add the default config layers.
        let sender = sender.layer([
            set_retry_policy(self.policies.retry),
            set_header(AppVersionHeader::new(self.app.app_version())),
            set_header(UserAgentHeader::new(self.app.user_agent())),
        ]);

        // Add the user's front layers.
        let sender = sender.layer(self.layers.front);

        // Build the client.
        Ok(Client::from_parts(sender, self.stores))
    }
}
