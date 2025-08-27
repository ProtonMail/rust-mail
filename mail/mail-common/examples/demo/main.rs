#![allow(clippy::print_stdout)]

use crate::cli::Cli;
use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[macro_use]
extern crate anyhow;

#[macro_use]
extern crate cfg_if;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate tracing;

mod app;
mod cli;
mod keychain;
mod notifier;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    cfg_if! {
        if #[cfg(feature = "gtk")] {
            main_gtk().await
        } else {
            main_cli().await
        }
    }
}

#[cfg(feature = "gtk")]
async fn main_gtk() -> Result<()> {
    use crate::app::backend::App;
    use futures::TryFutureExt;
    use proton_core_api::{session::Config, verification::ChallengeLoader};
    use tao::event_loop::EventLoopBuilder;

    let events = EventLoopBuilder::with_user_event().build();

    tokio::spawn(Cli::run(events.create_proxy()).inspect_err(|e| error!("{e:?}")));

    let config = Config {
        app_version: "ios-mail@7.1.0".to_owned(),
        ..Default::default()
    };

    let loader = ChallengeLoader::new(config).await?;

    App::new(&events, loader)?.run(events)
}

#[cfg(not(feature = "gtk"))]
async fn main_cli() -> Result<()> {
    use crate::app::events::Proxy;

    #[derive(Clone)]
    struct DummyProxy;

    impl Proxy for DummyProxy {}

    Cli::run(DummyProxy).await
}
